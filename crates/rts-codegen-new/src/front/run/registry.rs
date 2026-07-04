//! The REAL runtime Registry, built once and consulted for RUNTIME/Registry
//! class dispatch (Pilar 6 — data-driven dispatch, NO per-class metadata table).
//!
//! Per the PRIMORDIAL-vs-Registry doctrine the engine names ONLY the primordial
//! classes directly; everything else (Date/Map/Set/RegExp/Error family/the
//! Boolean/Number/String wrappers) lives in `rts-shared`/`rts-primitives`/
//! `rts-std` and publishes its surface — `(jsName, symbol, AbiType signature,
//! instanceof predicate)` — into an [`rts_engine::Registry`]. This module builds
//! that Registry once (the SAME `register`/`register_class_spec` fns the real
//! runtime uses, reached through the `rts-runtime` facade) and answers two
//! questions the lowering needs:
//!
//! - [`class_member`] — resolve `class.method(argc)` (or a static / getter) to a
//!   [`ResolvedCall`] (the real `__RTS_FN_*` symbol + its `AbiType` signature);
//! - [`class_ctors`] — every registered constructor overload's [`ResolvedCall`]
//!   (the generic ctor emitter picks by `[required, total]` arity + proven kind);
//! - [`instanceof_predicate`] — the real `fn(handle)->i64` tag symbol for
//!   `x instanceof class`, when the Registry declares one.
//!
//! The Registry is leaked to `'static` (built once at first use, never freed —
//! a compiler process lives and dies; this is the same lifetime model as the old
//! engine's `OnceLock<Registry>`). That lets a [`ResolvedCall`] borrow the real
//! `&'static str` symbol with no copy; the `AbiType`s are `Copy` so the small
//! `arg_abis` vec is the only allocation per resolution.

use std::sync::OnceLock;

use rts_engine::abi::{AbiType, DefaultArg, MemberFlags, MemberKind};
use rts_engine::{Engine, Member, Registry};

/// One resolved runtime call: the REAL `__RTS_FN_*` symbol + the exact `AbiType`
/// signature the generic marshal lowers against. For an INSTANCE method the
/// Registry's `Member.sig.args[0]` is the receiver `Handle`; we split it out as
/// [`recv_abi`](Self::recv_abi) so the generic emitter marshals the receiver
/// word + the explicit args uniformly.
#[derive(Clone)]
pub struct ResolvedCall {
    /// The canonical runtime symbol (`__RTS_FN_GL_DATE_GET_TIME`, …). `'static`
    /// because it borrows the leaked Registry's `Member.symbol`.
    pub symbol: &'static str,
    /// The receiver's `AbiType` for an instance method/getter (`args[0]`), or
    /// `None` for a constructor / static method (no implicit `this`).
    pub recv_abi: Option<AbiType>,
    /// The explicit (TS-visible) argument `AbiType`s, in order.
    pub arg_abis: Vec<AbiType>,
    /// The return `AbiType` (`Void` = no value).
    pub ret: AbiType,
    /// The member's modifier flags from the spec. Carries `MemberFlags::UNSOUND`
    /// for members that resolve but are NOT sound to lower (the honesty floor as
    /// spec DATA — the lowering bails on them generically, no per-class predicate).
    pub flags: MemberFlags,
    /// Per-arg default policy from the spec (parallel to the FULL `sig.args`,
    /// receiver included for instance methods; empty = every arg required). Lets
    /// the generic static/ctor emitter pad an omitted trailing arg with the
    /// spec-declared default (e.g. `Date.UTC`'s day=1/rest=0) instead of a codegen
    /// `pad_*` table. See [`super::registryclass`].
    pub default_args: Vec<DefaultArg>,
    /// For a `ret == Handle`: whether that handle is a HEAP STRING (`gc.*`,
    /// `string.*`, `buffer.to_string`) vs an OPAQUE RESOURCE handle (`audio.*`,
    /// `buffer.alloc`, `net.*` — a raw `u64` id from the namespace's own table,
    /// NOT a GC handle). A string-handle reboxes as `TAG_STR`; a resource-handle
    /// reboxes as a plain INTEGER PolyValue (its TS type is `number`). Derived
    /// from the member's `ts_signature` return type (`): string` ⇒ string handle).
    /// Irrelevant when `ret != Handle`.
    pub ret_is_string_handle: bool,
    /// For a `ret == Handle`: the `ts_signature` return type is an ARRAY (`): T[]`,
    /// e.g. `fs.readdir(): string[]`) — the handle is a FRESH JS `Entry::Vec` the
    /// namespace built for this call, so it reboxes through the heterogeneous
    /// authority (`__rtsadp_box_handle_auto`), which also normalizes legacy raw
    /// string-handle elements to words. Data-driven: any member declaring a `[]`
    /// return gets the array rebox — no per-member codegen path.
    pub ret_is_array_handle: bool,
    /// For a `ret == Handle` string return whose ts_signature is a NULLABLE union
    /// (`: string | null` / `: string | undefined`): a `0` handle means the value is
    /// ABSENT and must rebox as `null` (JS `URLSearchParams.get(missing)` etc.), NOT
    /// the empty string. An empty string `""` interns to a NON-zero handle, so `0`
    /// is unambiguously the absent sentinel here. `false` for a bare `: string`.
    pub ret_is_nullable_string: bool,
    /// For a `ret == Handle` that is an OBJECT instance of a NAMED runtime class
    /// (e.g. `ArrayBuffer.slice(): ArrayBuffer`, `URL.searchParams: URLSearchParams`):
    /// the class name parsed from the `ts_signature` return type. Lets a
    /// `const a = buf.slice(..)` record `a`'s class so a chained `a.byteLength`
    /// dispatches. `None` for primitives / generic `object` / unions. The caller
    /// still checks it against the registered classes (never invents one).
    pub ret_class: Option<String>,
}

/// Whether a member's `ts_signature` declares a `string` RETURN — used to tell a
/// heap-string `Handle` (rebox as `TAG_STR`) from an opaque resource `Handle`
/// (rebox as a raw integer `number`). Looks at the text after the LAST `):`.
/// The declared RETURN-TYPE text of a `ts_signature` — `"m(a: T): R"` → `"R"`,
/// or the GETTER form `"prop: R"` (no parens) → `"R"`. `None` when neither
/// shape yields a type text.
fn ts_ret_text(ts: &str) -> Option<&str> {
    let ret = match ts.rsplit_once("):") {
        Some((_, ret)) => ret,
        // Getter form `readonly prop: Type` — no parens; the FIRST `:` splits
        // name from type (a type can itself contain `:` only in exotic forms
        // the specs don't use).
        None => ts.split_once(':').map(|(_, r)| r)?,
    };
    Some(ret.trim().trim_end_matches(';').trim())
}

fn ts_returns_string(ts: &str) -> bool {
    ts_ret_text(ts)
        .map(|ret| {
            // A bare `string` OR a union with a `string` member (`string | null`,
            // `string | undefined` — a present value is a heap string, an absent
            // one is the 0 sentinel). Splitting on `|` keeps `get(): string | null`
            // a STRING handle instead of mis-reboxing it as an OBJECT.
            ret.split('|').any(|t| t.trim() == "string")
        })
        .unwrap_or(false)
}

/// Whether a member's `ts_signature` declares a NULLABLE string return — a union
/// of `string` with `null`/`undefined` (`get(...): string | null`). Lets the
/// rebox map a `0` (absent) handle to `null` instead of the empty string.
/// Whether a member's `ts_signature` declares an ARRAY return (`): string[]`,
/// `): number[]`). Routes the Handle rebox through `__rtsadp_box_handle_auto`.
fn ts_returns_array(ts: &str) -> bool {
    ts_ret_text(ts).is_some_and(|ret| ret.ends_with("[]"))
}

fn ts_returns_nullable_string(ts: &str) -> bool {
    ts.rsplit_once("):")
        .map(|(_, ret)| {
            let parts: Vec<&str> = ret.trim().trim_end_matches(';').split('|').map(str::trim).collect();
            parts.iter().any(|t| *t == "string")
                && parts.iter().any(|t| *t == "null" || *t == "undefined")
        })
        .unwrap_or(false)
}

/// The RETURN-TYPE NAME a member's `ts_signature` declares, when it is a single
/// class-like identifier (`slice(...): ArrayBuffer` → `Some("ArrayBuffer")`).
/// `None` for primitives (`number`/`string`/`boolean`/`void`/`object`), unions
/// (`object | undefined`), generics, and getters with no `):`. The caller must
/// still check the name against the registered classes before trusting it — this
/// only filters the SHAPE of the annotation, never invents a class.
fn ts_return_class(ts: &str) -> Option<String> {
    let ret = ts_ret_text(ts)?;
    // A GENERIC return (`Promise<T>`) names the class before the `<`.
    let ret = ret.split('<').next().unwrap_or(ret).trim();
    let first = ret.chars().next()?;
    if !first.is_ascii_uppercase() || !ret.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some(ret.to_string())
}

/// Build the real runtime Registry once. Only the classes the new engine
/// dispatches are registered (cheap — each `register*` just pushes metadata);
/// the externs themselves live in the linked runtime, reached by symbol.
fn build_registry() -> Registry {
    let mut e = Engine::new();
    // The full population (namespaces + class specs + `.ts` preludes) is data —
    // two tables in `registry_build`, iterated in order. Adding a registrar is one
    // row there, NOT an edit in the middle of this function (the old conflict trap).
    super::registry_build::populate(&mut e);
    e.into_registry()
}

/// The leaked, process-lifetime Registry. Built on first use.
fn registry() -> &'static Registry {
    static REG: OnceLock<&'static Registry> = OnceLock::new();
    REG.get_or_init(|| Box::leak(Box::new(build_registry())))
}

/// The embedded stdlib TS sources (engine `include`s), concatenated as one
/// declarations-only prelude string. Empty when nothing is embedded.
pub fn includes_prelude() -> String {
    registry().includes().join("\n")
}

/// Build a [`ResolvedCall`] from a resolved [`Member`], treating it as an
/// instance method (the receiver is `args[0]`).
fn instance_call(m: &'static Member) -> ResolvedCall {
    let mut args = m.sig.args.iter().copied();
    let recv_abi = args.next();
    // `default_args` is declared over ALL sig params (receiver included);
    // `arg_abis` drops the receiver, so the defaults must drop slot 0 too or
    // every explicit-arg default reads one position off.
    let default_args = if recv_abi.is_some() && !m.sig.default_args.is_empty() {
        m.sig.default_args[1..].to_vec()
    } else {
        m.sig.default_args.clone()
    };
    ResolvedCall {
        symbol: m.symbol.as_str(),
        recv_abi,
        arg_abis: args.collect(),
        ret: m.sig.returns,
        flags: m.flags,
        default_args,
        ret_is_string_handle: ts_returns_string(&m.ts_signature),
        ret_is_array_handle: ts_returns_array(&m.ts_signature),
        ret_is_nullable_string: ts_returns_nullable_string(&m.ts_signature),
        ret_class: ts_return_class(&m.ts_signature),
    }
}

/// Build a [`ResolvedCall`] for a constructor / static method (no implicit
/// receiver — every `args` entry is an explicit parameter).
fn flat_call(m: &'static Member) -> ResolvedCall {
    ResolvedCall {
        symbol: m.symbol.as_str(),
        recv_abi: None,
        arg_abis: m.sig.args.clone(),
        ret: m.sig.returns,
        flags: m.flags,
        default_args: m.sig.default_args.clone(),
        ret_is_string_handle: ts_returns_string(&m.ts_signature),
        ret_is_array_handle: ts_returns_array(&m.ts_signature),
        ret_is_nullable_string: ts_returns_nullable_string(&m.ts_signature),
        ret_class: ts_return_class(&m.ts_signature),
    }
}

/// Whether the Registry knows `class` as a RUNTIME/Registry class the engine can
/// construct + dispatch through here.
pub fn has_class(class: &str) -> bool {
    registry().class(class).is_some()
}

/// Every `(symbol, fn_ptr)` of the registered namespace `rts:<ns>` whose member
/// carries a REAL (non-null) function pointer — the JIT-installable symbols of
/// that namespace. The JIT must install ALL of them (not just the handful the
/// prelude bundle calls): once a namespace is registered, ANY of its members is
/// resolvable via [`namespace_member`] (`import { byte_len } from "rts:string"`),
/// so each emitted `call <symbol>` needs its address installed or it is a
/// link-OK/runtime-SIGILL (the honesty floor's "nothing that crashes as pass").
/// Null pointers (alias/external members) are skipped — same null-skip invariant
/// as the engine's own `jit_symbols`.
pub fn namespace_jit_symbols(ns: &str) -> Vec<(&'static str, *const u8)> {
    let key = format!("rts:{ns}");
    let Some(m) = registry().module(&key) else {
        return Vec::new();
    };
    m.members
        .iter()
        .filter(|mem| !mem.fn_ptr.0.is_null())
        .map(|mem| (mem.symbol.as_str(), mem.fn_ptr.0))
        .collect()
}

/// EVERY `(symbol, fn_ptr)` the Registry harvested across ALL registered
/// namespaces + classes (the engine's null-skipped `jit_symbols` table). The JIT
/// installs all of them so every member resolvable via [`namespace_member`] /
/// [`class_member`] has its address present — the SIGILL-safe wholesale install
/// for the macro-`#[rts_namespace]` surfaces (their members carry real `fn_ptr`s).
/// Namespaces whose members carry NULL `fn_ptr`s (gc pool / `string`) are absent
/// here and installed by address in `runtime_link`.
pub fn all_jit_symbols() -> Vec<(&'static str, *const u8)> {
    registry()
        .jit_symbols()
        .filter(|(_, p)| !p.0.is_null())
        .map(|(s, p)| (s, p.0))
        .collect()
}

/// Whether `ns` is a registered builtin NAMESPACE (`rts:<ns>`). Lets the lowering
/// recognize a bare ambient namespace ident (`test_core`/`string`/`fmt`) used by
/// the embedded prelude and route it through [`namespace_member`]. Empty `ns`
/// (bare `"rts"`) is not a namespace.
pub fn has_namespace(ns: &str) -> bool {
    !ns.is_empty() && registry().module(&format!("rts:{ns}")).is_some()
}

/// Resolve `class.method(argc)` as an INSTANCE method to its [`ResolvedCall`].
/// `argc` is the EXPLICIT (TS-visible) arg count; the Registry's resolver honours
/// overload-by-arity + variadics. `None` when the class/method is unknown.
pub fn class_member(class: &str, method: &str, argc: usize) -> Option<ResolvedCall> {
    let c = registry().class(class)?;
    let m = c.resolve_instance_method(method, argc)?;
    Some(instance_call(m))
}

/// The provable RETURN CLASS of instance method `class.method`, ANY arity —
/// first same-named instance method whose ts signature names a registered
/// class (`then(cb): Promise` → "Promise"). Lets a CHAINED receiver classify
/// (`Promise.resolve(4).then(cb)` — the outer `.then`'s receiver class).
pub fn class_member_ret_class(class: &str, method: &str) -> Option<String> {
    let c = registry().class(class)?;
    c.members
        .iter()
        .filter(|m| matches!(m.kind, MemberKind::InstanceMethod) && m.matches_name(method))
        .find_map(|m| ts_return_class(&m.ts_signature))
        .filter(|rc| has_class(rc))
}

/// The provable RETURN CLASS of instance GETTER `class.prop` — the getter's ts
/// signature naming a registered class (`readonly searchParams: URLSearchParams`).
/// Lets a CHAINED getter receiver classify (`url.searchParams.get(..)` without
/// the intermediate local).
pub fn class_getter_ret_class(class: &str, prop: &str) -> Option<String> {
    let c = registry().class(class)?;
    // A property may be modeled as a real InstanceGetter OR as a recv-only
    // InstanceMethod read as a member (URL's `searchParams`) — accept both.
    let m = c
        .instance_getter(prop)
        .or_else(|| c.resolve_instance_method(prop, 0))?;
    ts_return_class(&m.ts_signature).filter(|rc| has_class(rc))
}

/// Resolve `inst.prop` as an INSTANCE GETTER (a `MemberKind::InstanceGetter`, read
/// with no `()`) to its [`ResolvedCall`]. `None` when the class has no such getter.
/// (Distinct from `class_member`, which only matches `InstanceMethod` — some
/// classes model a property as a real getter, e.g. `Symbol.prototype.description`.)
pub fn class_instance_getter(class: &str, prop: &str) -> Option<ResolvedCall> {
    let c = registry().class(class)?;
    let m = c.instance_getter(prop)?;
    Some(instance_call(m))
}

/// Resolve a STATIC method `Class.method` by NAME ALONE — ANY arity — to its
/// [`ResolvedCall`]. The generic static emitter then validates the caller's argc
/// against the member's `[required, total]` window (derived from `default_args`)
/// and pads the omitted tail. Unique-named statics (Date's now/parse/UTC) resolve
/// first-match; `None` when the class/static is unknown.
pub fn class_static_any(class: &str, method: &str) -> Option<ResolvedCall> {
    let c = registry().class(class)?;
    let m = c.members.iter().find(|m| {
        matches!(m.kind, MemberKind::StaticMethod | MemberKind::Function) && m.matches_name(method)
    })?;
    Some(flat_call(m))
}

/// A zero-arg STATIC CONSTANT member of a Registry class (`Symbol.iterator`,
/// well-known symbols): `MemberKind::Constant` on the class spec, read as a
/// member (not called). Resolved to its zero-arg getter call.
pub fn class_static_const(class: &str, name: &str) -> Option<ResolvedCall> {
    let c = registry().class(class)?;
    let m = c
        .members
        .iter()
        .find(|m| matches!(m.kind, MemberKind::Constant) && m.matches_name(name))?;
    Some(flat_call(m))
}

/// EVERY static/function overload of `method` on `class`, in declaration order —
/// lets the static-call lowering pick the overload whose arity window admits the
/// caller's argc (e.g. `URL.canParse(href)` vs `URL.canParse(href, base)`, two
/// same-named statics of arity 1 and 2). `class_static_any` returns only the FIRST
/// match, which wrongly rejects the higher-arity overloads.
pub fn class_statics(class: &str, method: &str) -> Vec<ResolvedCall> {
    let Some(c) = registry().class(class) else {
        return Vec::new();
    };
    c.members
        .iter()
        .filter(|m| {
            matches!(m.kind, MemberKind::StaticMethod | MemberKind::Function)
                && m.matches_name(method)
        })
        .map(|m| flat_call(m))
        .collect()
}

/// Every constructor's argument-`AbiType` slice for `class`, in declaration
/// order — lets the ctor lowering pick the overload by arg TYPE (Date's 1-arg
/// ms-vs-ISO split: a `[I64]` ctor vs a `[StrPtr]` ctor of the same arity).
pub fn class_ctors(class: &str) -> Vec<ResolvedCall> {
    let Some(c) = registry().class(class) else {
        return Vec::new();
    };
    c.members
        .iter()
        .filter(|m| matches!(m.kind, MemberKind::Constructor))
        .map(|m| flat_call(m))
        .collect()
}

/// Resolve a BUILTIN-IMPORT namespace member (`import { member } from "rts:<ns>"`)
/// to its [`ResolvedCall`] — the real `__RTS_FN_NS_*` symbol + its `AbiType`
/// signature, marshaled through the SAME generic path as a class method
/// ([`super::registry_call`]), only with NO receiver (`recv_abi: None`: a namespace
/// function has no implicit `this`; every `Sig::args` entry is an explicit param).
///
/// `argc` is the EXPLICIT (TS-visible) arg count; a member whose `Sig::args.len()`
/// does not match it is rejected (`None`) so the call lowering bails honestly
/// rather than mis-marshaling. Only `MemberKind::Function`/`StaticMethod` members
/// are resolvable (a namespace exposes plain functions); constants/getters are not
/// callable and return `None`. Bare `"rts"` (`ns == ""`) is NOT handled here — it
/// imports a namespace OBJECT, a different shape the caller bails on.
pub fn namespace_member(ns: &str, member: &str, argc: usize) -> Option<ResolvedCall> {
    if ns.is_empty() {
        return None;
    }
    let m = registry().module(&format!("rts:{ns}"))?;
    let is_callable = |m: &&Member| {
        matches!(m.kind, MemberKind::Function | MemberKind::StaticMethod) && m.matches_name(member)
    };
    let found = m
        .members
        .iter()
        .find(|m| is_callable(m) && m.sig.args.len() == argc)
        .or_else(|| {
            m.members
                .iter()
                .find(|m| is_callable(m) && m.variadic && argc >= m.sig.args.len().saturating_sub(1))
        })?;
    Some(flat_call(found))
}

/// Resolve a namespace CONSTANT (`math.PI`, `math.E` — `MemberKind::Constant`,
/// a zero-arg getter symbol) to its [`ResolvedCall`]. `None` when the namespace
/// has no such constant (the caller falls through / bails).
pub fn namespace_const(ns: &str, name: &str) -> Option<ResolvedCall> {
    if ns.is_empty() {
        return None;
    }
    let m = registry().module(&format!("rts:{ns}"))?;
    let found = m
        .members
        .iter()
        .find(|m| matches!(m.kind, MemberKind::Constant) && m.matches_name(name))?;
    Some(flat_call(found))
}

/// The real `instanceof` predicate symbol (`fn(handle)->i64`) the Registry
/// declares for `class`, or `None` (then the engine has no Registry-driven
/// instanceof for it and falls back to its tag check / bail).
pub fn instanceof_predicate(class: &str) -> Option<&'static str> {
    // `registry()` returns `&'static Registry`, so every borrow off it is itself
    // `'static` — no unsafe needed.
    let c: &'static rts_engine::Class = registry().class(class)?;
    c.instanceof_predicate.as_deref()
}

/// The candidate set for the DYNAMIC (runtime-predicate) Registry instance
/// GETTER dispatch: every Registry class that declares an `instanceof_predicate`
/// AND an instance getter `prop`. Data-driven — the front names no class.
pub fn dyn_getter_candidates(prop: &str) -> Vec<(String, &'static str, ResolvedCall)> {
    let reg: &'static rts_engine::Registry = registry();
    reg.classes()
        .filter_map(|c| {
            let pred = c.instanceof_predicate.as_deref()?;
            let m = c.instance_getter(prop)?;
            Some((c.name.clone(), pred, instance_call(m)))
        })
        .collect()
}

/// The candidate set for the DYNAMIC (runtime-predicate) Registry instance
/// dispatch: every Registry class that BOTH declares an `instanceof_predicate`
/// AND resolves instance method `method` for a call with `argc` explicit args
/// (arity window `[required, total]`, same rule the static instance path uses).
/// Ordered as registered. Data-driven — the front names no class.
pub fn dyn_instance_candidates(
    method: &str,
    argc: usize,
) -> Vec<(String, &'static str, ResolvedCall)> {
    let reg: &'static rts_engine::Registry = registry();
    reg.classes()
        .filter_map(|c| {
            let pred = c.instanceof_predicate.as_deref()?;
            let m = c.resolve_instance_method(method, argc)?;
            let call = instance_call(m);
            // The call must ADMIT argc: within [required, total] of the explicit
            // params (the emitter pads the omitted defaulted tail). Same rule as
            // `required_args` in registryclass.rs: an empty `default_args` means
            // every param is required.
            let total = call.arg_abis.len();
            let required = if call.default_args.is_empty() {
                total
            } else {
                call.default_args
                    .iter()
                    .take_while(|d| matches!(d, DefaultArg::Required))
                    .count()
            };
            if argc < required || argc > total {
                return None;
            }
            Some((c.name.clone(), pred, call))
        })
        .collect()
}
