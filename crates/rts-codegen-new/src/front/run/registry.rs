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
//! - [`class_ctor`] — resolve `new class(argc)` to the constructor's
//!   [`ResolvedCall`];
//! - [`instanceof_predicate`] — the real `fn(handle)->i64` tag symbol for
//!   `x instanceof class`, when the Registry declares one.
//!
//! The Registry is leaked to `'static` (built once at first use, never freed —
//! a compiler process lives and dies; this is the same lifetime model as the old
//! engine's `OnceLock<Registry>`). That lets a [`ResolvedCall`] borrow the real
//! `&'static str` symbol with no copy; the `AbiType`s are `Copy` so the small
//! `arg_abis` vec is the only allocation per resolution.

use std::sync::OnceLock;

use rts_engine::abi::{AbiType, MemberKind};
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
    // The PRIMORDIAL `Boolean.prototype` methods as faithful TS (embedded include):
    // its ambient `class Boolean` supplies `toString`/`valueOf`. A method called on
    // a PRIMITIVE bool receiver (`true.toString()`) is routed into this class with
    // the primitive boxed as `this` (see `method::try_primitive_class_method`). The
    // `new Boolean(x)` WRAPPER (typeof === "object") stays the engine's wrapper
    // trampoline — the lowering keeps `Boolean` a global-class ctor regardless of
    // this prelude class (see `is_global_class_ctor`).
    e.include(rts_runtime::BOOLEAN_TS);
    // The faithful TS Map/Set stdlib (embedded include): its ambient `class Map`/
    // `class Set` shadow the native dispatch in every program — making the native
    // Map/Set code dead (deleted in B3).
    e.include(rts_runtime::stdlib::MAP_SET_TS);
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
    }
}

/// Whether the Registry knows `class` as a RUNTIME/Registry class the engine can
/// construct + dispatch through here.
pub fn has_class(class: &str) -> bool {
    registry().class(class).is_some()
}

/// Resolve `class.method(argc)` as an INSTANCE method to its [`ResolvedCall`].
/// `argc` is the EXPLICIT (TS-visible) arg count; the Registry's resolver honours
/// overload-by-arity + variadics. `None` when the class/method is unknown.
pub fn class_member(class: &str, method: &str, argc: usize) -> Option<ResolvedCall> {
    let c = registry().class(class)?;
    let m = c.resolve_instance_method(method, argc)?;
    Some(instance_call(m))
}

/// Resolve a STATIC method `Class.method(argc)` to its [`ResolvedCall`]. The
/// Registry stores statics as `MemberKind::StaticMethod` (or, for Date's
/// now/parse/UTC, as `Function`); accept either, distinguishing by name + the
/// explicit `argc` honouring variadic tails.
pub fn class_static(class: &str, method: &str, argc: usize) -> Option<ResolvedCall> {
    let c = registry().class(class)?;
    let is_static = |m: &&Member| {
        matches!(m.kind, MemberKind::StaticMethod | MemberKind::Function) && m.matches_name(method)
    };
    let m = c
        .members
        .iter()
        .find(|m| is_static(m) && m.sig.args.len() == argc)
        .or_else(|| {
            c.members
                .iter()
                .find(|m| is_static(m) && m.variadic && argc >= m.sig.args.len().saturating_sub(1))
        })
        .or_else(|| c.members.iter().find(is_static))?;
    Some(flat_call(m))
}

/// Resolve `new class(argc)` to the matching constructor's [`ResolvedCall`],
/// distinguishing overloads by the EXPLICIT arg count (Date has 0/1-num/1-str/
/// 7-field forms). `None` when no constructor matches that arity.
pub fn class_ctor(class: &str, argc: usize) -> Option<ResolvedCall> {
    let c = registry().class(class)?;
    let m = c
        .members
        .iter()
        .find(|m| matches!(m.kind, MemberKind::Constructor) && m.sig.args.len() == argc)?;
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
