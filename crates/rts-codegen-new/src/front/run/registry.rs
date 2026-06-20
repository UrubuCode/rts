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
}

/// Build the real runtime Registry once. Only the classes the new engine
/// dispatches are registered (cheap — each `register*` just pushes metadata);
/// the externs themselves live in the linked runtime, reached by symbol.
fn build_registry() -> Registry {
    use rts_runtime::namespaces as ns;
    let mut e = Engine::new();
    // Namespaces backing the class statics/ctors (Date.now/UTC/parse use the
    // `date` namespace; the collections map/vec back Map/Set).
    ns::date::register(&mut e);
    ns::collections::register(&mut e);
    ns::regex::register(&mut e);
    // BUILTIN-IMPORT namespaces (`import { print } from "rts:io"`): the public
    // std surface the new engine resolves through [`namespace_member`]. Each
    // `register` pushes the namespace's member metadata (name + symbol + ABI Sig)
    // into the Registry; the call lowering resolves a `Binding::Builtin{ns,member}`
    // to the real `__RTS_FN_NS_*` symbol from here. The JIT addresses are installed
    // by `runtime_link::jit_symbols` (io/math symbols already linked there).
    ns::io::register(&mut e);
    ns::math::register(&mut e);
    // `gc` — the string-pool / handle surface (`gc.string_from_i64`/`string_free`/…)
    // the rts:test fixtures use directly. Its `__RTS_FN_NS_GC_*` symbols are JIT-
    // linked in `runtime_link`.
    ns::gc::register(&mut e);
    // The `rts:test` FRAMEWORK backing namespaces, used AMBIENTLY by the embedded
    // `TEST_BUNDLE_TS` prelude (the high-level describe/test/expect harness):
    // `test_core.*` (the runner primitives), `string.*` (contains/startsWith/
    // endsWith for the matchers), `fmt.parse_f64` (numeric matchers). They are
    // resolved as bare-ambient namespace calls gated to PRELUDE-origin code
    // (`method::try_method_dispatch`), and their `__RTS_FN_NS_{TEST_CORE,STRING,FMT}_*`
    // symbols are JIT-linked in `runtime_link`.
    ns::test::register(&mut e);
    ns::globals::string::register(&mut e);
    ns::fmt::register(&mut e);
    // The broad std namespace surface the `tests/*.test.ts` import via `rts:<ns>`.
    // Registering each makes its members resolvable through [`namespace_member`];
    // their `__RTS_FN_NS_*` symbols carry REAL `fn_ptr`s (macro-`#[rts_namespace]`),
    // so [`all_jit_symbols`] harvests + installs the whole surface in one shot —
    // SIGILL-safe (no link-OK/runtime-miss). Heavier/feature-gated namespaces
    // (http_server/tls/ui/audio/runtime) are deliberately out until a test needs them.
    ns::fs::register(&mut e);
    ns::time::register(&mut e);
    ns::env::register(&mut e);
    ns::path::register(&mut e);
    ns::num::register(&mut e);
    ns::mem::register(&mut e);
    ns::hash::register(&mut e);
    ns::hint::register(&mut e);
    ns::ptr::register(&mut e);
    ns::buffer::register(&mut e);
    ns::alloc::register(&mut e);
    ns::bigfloat::register(&mut e);
    ns::atomic::register(&mut e);
    ns::sync::register(&mut e);
    ns::trace::register(&mut e);
    ns::process::register(&mut e);
    ns::os::register(&mut e);
    ns::crypto::register(&mut e);
    ns::net::register(&mut e);
    ns::json::register(&mut e);
    ns::promise::register(&mut e);
    ns::parallel::register(&mut e);
    ns::thread::register(&mut e);
    ns::ffi::register(&mut e);
    ns::events::register(&mut e);
    // The PRIVATE `engine` namespace (arch/time/trace) the embedded TS prelude
    // calls. Marked `.private()`; the lowering's `engineobj` gate enforces that
    // only prelude-origin code names the `engine` global.
    ns::engine::register(&mut e);
    // The RUNTIME/Registry global classes the engine constructs + dispatches.
    ns::globals::date::register_class_spec(&mut e);
    ns::globals::regexp::register_regexp_class_spec(&mut e);
    // The Error family is NOT registered here: it is a PRIMORDIAL `.ts` prelude
    // class (`ERROR_TS`, included below). A `new Error("x")` constructs that
    // user-class shape; nothing in the new engine consults a Registry `Error`
    // class. (The Rust `globals::error` runtime stays — the FROZEN old engine
    // `rts-codegen-old` still registers + uses it.)
    ns::globals::boolean::register_boolean_class_spec(&mut e);
    // Number/String classes also exist as primordials; we register them so the
    // wrapper-ctor (`new Number(x)`) path can resolve through the Registry too.
    ns::globals::number::register_number_class_spec(&mut e);
    ns::globals::string::register_string_class_spec(&mut e);
    // The PRIMORDIAL `Error` family as faithful TS (embedded include): its ambient
    // `class Error` (+ TypeError/RangeError/… subclasses) construct as shape-based
    // objects exactly like a user class — replacing the former hardcoded codegen
    // synth + `__rtsadp_err_*` trampolines. Real `.stack` via `engine.trace_capture()`.
    // Included BEFORE Map/Set so the error subclasses (`extends Error`) see the
    // `Error` base first (the prelude is one merged program; `includes()` joins them
    // in registration order, so order here is declaration order).
    e.include(rts_runtime::ERROR_TS);
    // The PRIMORDIAL `Object` instance-method library + factory as faithful TS
    // (`OBJECT_TS`): its ambient `class Object` supplies `hasOwnProperty`/
    // `propertyIsEnumerable`/`toString`/`valueOf`. An object-receiver method call
    // (`o.hasOwnProperty(k)`) routes into this class with the object as `this` (see
    // `method::try_method_dispatch`'s OBJECT-receiver branch); membership rides the
    // shape-aware `engine.obj_has` bridge. The STATIC surface (`Object.keys`/…)
    // stays codegen-native in `objstatic.rs` and is transparent to this class (the
    // `name != "Object"` carve-out there).
    e.include(rts_runtime::OBJECT_TS);
    // The PRIMORDIAL `Boolean.prototype` methods as faithful TS (embedded include):
    // its ambient `class Boolean` supplies `toString`/`valueOf`. A method called on
    // a PRIMITIVE bool receiver (`true.toString()`) is routed into this class with
    // the primitive boxed as `this` (see `method::try_primitive_class_method`). The
    // `new Boolean(x)` WRAPPER (typeof === "object") stays the engine's wrapper
    // trampoline — the lowering keeps `Boolean` a global-class ctor regardless of
    // this prelude class (see `is_global_class_ctor`).
    e.include(rts_runtime::BOOLEAN_TS);
    // The PRIMORDIAL `Number.prototype` methods as faithful TS (embedded include):
    // its ambient `class Number` supplies `valueOf`/`toString(radix?)`/`toFixed`/
    // `toPrecision`/`toExponential`/`toLocaleString`. A method called on a PRIMITIVE
    // number receiver (`(5).toFixed(2)`) routes into this class with the primitive
    // boxed as `this` (see `method::try_primitive_class_method`). The irreducible
    // numeric FORMATTING stays in Rust and is bridged via the private `engine.num_*`
    // helpers the `.ts` bodies call (one source of truth). The `new Number(x)`
    // WRAPPER (typeof === "object") stays the engine's wrapper trampoline (see
    // `is_global_class_ctor` / `is_wrapper_primordial`).
    e.include(rts_runtime::NUMBER_TS);
    // The PRIMORDIAL `String.prototype` methods as faithful TS (embedded include):
    // its ambient `class String` supplies case/trim/charAt/charCodeAt/at/repeat/
    // slice/substring/indexOf/lastIndexOf/includes/startsWith/endsWith/padStart/
    // padEnd/concat/replace/replaceAll. A method called on a PRIMITIVE string
    // receiver (`"abc".toUpperCase()`) routes into this class with the primitive
    // boxed as `this` (see `method::try_primitive_class_method`). The irreducible
    // Unicode string logic stays in Rust and is bridged via the private
    // `engine.str_*` helpers the `.ts` bodies call (one source of truth). `.length`
    // stays an engine direct read; `split` (array) + the regex-first methods stay
    // on the engine's dispatch paths. The `new String(x)` WRAPPER (typeof ===
    // "object") stays the engine's wrapper trampoline (see `is_global_class_ctor` /
    // `is_wrapper_primordial`).
    e.include(rts_runtime::STRING_TS);
    // The faithful TS Map/Set stdlib (embedded include): its ambient `class Map`/
    // `class Set` shadow the native dispatch in every program — making the native
    // Map/Set code dead (deleted in B3).
    e.include(rts_runtime::stdlib::MAP_SET_TS);
    // The `JSON` stdlib (stringify/parse) as faithful TS — pure primordials, the
    // engine names nothing JSON-specific. Its ambient `class JSON` static methods
    // serve `JSON.stringify(x)` / `JSON.parse(s)`.
    e.include(rts_runtime::stdlib::JSON_TS);
    // The high-level `rts:test` FRAMEWORK as faithful TS (embedded include): its
    // ambient `describe`/`test`/`expect`/`Matcher` (+ lifecycle hooks) are the
    // surface every `tests/*.test.ts` imports from `"rts:test"`. Included LAST so
    // its `class Matcher` + functions see every primordial already ambient. The
    // `import { describe, test, expect } from "rts:test"` in a test file binds to
    // these prelude declarations (see `flatten`: `rts:test` names resolve to the
    // ambient prelude function of the same name, NOT a namespace member). The
    // bundle's bare `test_core.*`/`string.*`/`fmt.*` calls resolve as prelude-only
    // ambient namespace calls (`method::try_method_dispatch`).
    e.include(rts_runtime::namespaces::test::BUNDLE_TS);
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
    ResolvedCall {
        symbol: m.symbol.as_str(),
        recv_abi,
        arg_abis: args.collect(),
        ret: m.sig.returns,
        flags: m.flags,
        default_args: m.sig.default_args.clone(),
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

/// The real `instanceof` predicate symbol (`fn(handle)->i64`) the Registry
/// declares for `class`, or `None` (then the engine has no Registry-driven
/// instanceof for it and falls back to its tag check / bail).
pub fn instanceof_predicate(class: &str) -> Option<&'static str> {
    // `registry()` returns `&'static Registry`, so every borrow off it is itself
    // `'static` — no unsafe needed.
    let c: &'static rts_engine::Class = registry().class(class)?;
    c.instanceof_predicate.as_deref()
}
