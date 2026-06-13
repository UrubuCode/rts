//! Data-driven method dispatch — the "no builtins in the engine" rule.
//!
//! The engine may name ONLY primordial classes (String/Object/Array/Function/
//! Promise/Boolean/Number/Error+subclasses). A method call `recv.method(args)`
//! resolves through class METADATA — not a per-method `if method == "..."`
//! switchboard — to a REAL runtime `__RTS_FN_*` symbol + its lowered signature,
//! which the lowering then marshals + `call`s. ONE generic emit path.
//!
//! ## Why a hand-written metadata table (and why it is still honest)
//!
//! The real runtime's class metadata lives in a `Registry` built by calling the
//! `register_*_class_spec(&mut Engine)` builders. Those builders, and the
//! `Engine`/`Registry`/`Class` types, are NOT re-exported through the
//! `rts-runtime` FACADE (only `rts_engine::abi::*` is), and the crate-layering
//! rule forbids adding `rts-engine`/`rts-primitives` as a second direct
//! dependency (see `runtime_link.rs` for the same constraint). So — exactly like
//! [`crate::value::abi_sig`] does for the string-pool symbols — this module
//! hand-writes a small static table covering the methods the lowering emits.
//!
//! It is NOT a switchboard and NOT invented symbols: every [`MethodSpec`]
//! references the ACTUAL `__RTS_FN_GL_*` extern the runtime defines, with the
//! ACTUAL `AbiType` signature the real class spec declares (verified against
//! `rts-primitives/src/string|number|array`). [`resolve_method`] is ONE generic
//! lookup keyed by `(class, method, argc)` — the same `(name, arity)` resolution
//! `Class::resolve_instance_method` performs — and the lowering ([`crate::front::run::method`])
//! marshals purely from the returned `params`/`ret` `AbiType`s. Adding a method
//! is a data row, never a code arm. At cutover this table is replaced by a real
//! registry harvest once the facade exposes one; the lowering does not change.

use rts_runtime::abi::AbiType;

/// The class implied by a receiver value, as the engine can prove it statically.
/// Only the receiver kinds the engine can name (primordials) appear here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RecvClass {
    /// A `string` primitive (a `TAG_STR` PolyValue): receiver marshals as a real
    /// string handle (`POLY_TO_HANDLE`).
    String,
    /// A `number` primitive (an int32 / double): receiver marshals as `F64`.
    Number,
}

/// How the receiver value is passed to the real symbol at slot 0.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RecvAbi {
    /// Slot 0 is the receiver's real GC handle (string/object), one `i64` slot.
    Handle,
    /// Slot 0 is the receiver coerced to `f64` (a Number primitive), one `f64`.
    F64,
}

/// A resolved instance-method target: the real runtime symbol + the explicit
/// (non-receiver) argument `AbiType`s + the return `AbiType`. The lowering reads
/// these to marshal each PolyValue arg and the result. The receiver convention is
/// `recv_abi`; the symbol's slot 0 is always the receiver.
#[derive(Clone, Copy, Debug)]
pub struct MethodSpec {
    /// The real `__RTS_FN_*` extern symbol the lowering `call`s.
    pub symbol: &'static str,
    /// How slot 0 (the receiver) is marshaled.
    pub recv_abi: RecvAbi,
    /// The explicit argument `AbiType`s, in order (slots after the receiver).
    /// A method with optional trailing args is registered once per supported
    /// arity (so `argc` matches a row exactly — no default injection here).
    pub args: &'static [AbiType],
    /// The return `AbiType` (`Handle` ⇒ a string/object handle the lowering
    /// re-boxes; `I64`/`F64` ⇒ a number; `Bool` ⇒ a boolean singleton).
    pub ret: AbiType,
}

/// Resolve `recv_class.method(argc explicit args)` to a real [`MethodSpec`], or
/// `None` when the metadata has no matching `(method, arity)` row. ONE generic
/// path: a linear scan of the class's static method table for a name + exact
/// explicit-arity match (mirroring `Class::resolve_instance_method`'s exact-arity
/// rule). `None` makes the lowering BAIL explicitly — never a guess.
pub fn resolve_method(recv_class: RecvClass, method: &str, argc: usize) -> Option<MethodSpec> {
    let rows: &[(&'static str, usize, MethodSpec)] = match recv_class {
        RecvClass::String => STRING_ROWS,
        RecvClass::Number => NUMBER_ROWS,
    };
    rows.iter()
        .find(|(name, arity, _)| *name == method && *arity == argc)
        .map(|(_, _, spec)| *spec)
}

// ===========================================================================
// String — instance methods (receiver = real string handle, slot 0).
//
// Symbols + signatures verified against rts-primitives/src/string/{mod,rt}.rs
// (register_string_class_spec). `Handle` args are real string handles; `I64`
// args are indices/counts; returns are `Handle` (string) / `I64` / `Bool`.
// Methods with optional trailing args are listed once per supported arity.
// ===========================================================================

use AbiType::{Bool, Handle, I64};

/// A String instance-method row: `(jsName, explicitArity, spec)`. Receiver is a
/// real string handle. Listed once per supported arity (an arity a real default
/// covers is listed explicitly so `argc` matches without default injection).
const STRING_ROWS: &[(&str, usize, MethodSpec)] = &[
    // ---- 0-arg, return string ----
    ("toUpperCase", 0, sm("__RTS_FN_GL_STRING_TO_UPPER_CASE", &[], Handle)),
    ("toLowerCase", 0, sm("__RTS_FN_GL_STRING_TO_LOWER_CASE", &[], Handle)),
    ("toLocaleUpperCase", 0, sm("__RTS_FN_GL_STRING_TO_UPPER_CASE", &[], Handle)),
    ("toLocaleLowerCase", 0, sm("__RTS_FN_GL_STRING_TO_LOWER_CASE", &[], Handle)),
    ("trim", 0, sm("__RTS_FN_GL_STRING_TRIM", &[], Handle)),
    ("trimStart", 0, sm("__RTS_FN_GL_STRING_TRIM_START", &[], Handle)),
    ("trimEnd", 0, sm("__RTS_FN_GL_STRING_TRIM_END", &[], Handle)),
    ("trimLeft", 0, sm("__RTS_FN_GL_STRING_TRIM_START", &[], Handle)),
    ("trimRight", 0, sm("__RTS_FN_GL_STRING_TRIM_END", &[], Handle)),
    // ---- index/count args (I64), return string ----
    ("charAt", 1, sm("__RTS_FN_GL_STRING_CHAR_AT", &[I64], Handle)),
    ("at", 1, sm("__RTS_FN_GL_STRING_AT", &[I64], Handle)),
    ("repeat", 1, sm("__RTS_FN_GL_STRING_REPEAT", &[I64], Handle)),
    // slice/substring/substr: only the 2-arg form (both indices explicit) is
    // registered. The 1-arg form relies on a runtime "to end" default that this
    // table does not inject, so `s.slice(n)` BAILS (a later increment).
    ("slice", 2, sm("__RTS_FN_GL_STRING_SLICE", &[I64, I64], Handle)),
    ("substring", 2, sm("__RTS_FN_GL_STRING_SUBSTRING", &[I64, I64], Handle)),
    ("substr", 2, sm("__RTS_FN_GL_STRING_SUBSTR", &[I64, I64], Handle)),
    // ---- string args (Handle), return number/bool ----
    ("indexOf", 1, sm("__RTS_FN_GL_STRING_INDEX_OF", &[Handle], I64)),
    ("lastIndexOf", 1, sm("__RTS_FN_GL_STRING_LAST_INDEX_OF", &[Handle], I64)),
    ("includes", 1, sm("__RTS_FN_GL_STRING_INCLUDES", &[Handle], Bool)),
    ("startsWith", 1, sm("__RTS_FN_GL_STRING_STARTS_WITH", &[Handle], Bool)),
    ("endsWith", 1, sm("__RTS_FN_GL_STRING_ENDS_WITH", &[Handle], Bool)),
    // ---- char code: index arg, return number ----
    ("charCodeAt", 1, sm("__RTS_FN_GL_STRING_CHAR_CODE_AT", &[I64], I64)),
    // ---- two string args, return string ----
    ("replace", 2, sm("__RTS_FN_GL_STRING_REPLACE", &[Handle, Handle], Handle)),
    ("replaceAll", 2, sm("__RTS_FN_GL_STRING_REPLACE_ALL", &[Handle, Handle], Handle)),
    ("concat", 1, sm("__RTS_FN_GL_STRING_CONCAT", &[Handle], Handle)),
    // ---- pad: target length + pad string ----
    ("padStart", 2, sm("__RTS_FN_GL_STRING_PAD_START", &[I64, Handle], Handle)),
    ("padEnd", 2, sm("__RTS_FN_GL_STRING_PAD_END", &[I64, Handle], Handle)),
];

// ===========================================================================
// Number — instance methods (receiver = the f64 primitive, slot 0).
//
// Verified against rts-primitives/src/number.rs (register_number_class_spec).
// ===========================================================================

/// A Number instance-method row. Receiver is the `f64` primitive (`RecvAbi::F64`).
const NUMBER_ROWS: &[(&str, usize, MethodSpec)] = &[
    ("toFixed", 1, nm("__RTS_FN_GL_NUMBER_TO_FIXED", &[I64], Handle)),
    ("toPrecision", 1, nm("__RTS_FN_GL_NUMBER_TO_PRECISION", &[I64], Handle)),
    ("toExponential", 1, nm("__RTS_FN_GL_NUMBER_TO_EXPONENTIAL", &[I64], Handle)),
    ("toString", 1, nm("__RTS_FN_GL_NUMBER_TO_STRING_RADIX", &[I64], Handle)),
];

/// Build a String instance-method spec (receiver = real string handle).
const fn sm(symbol: &'static str, args: &'static [AbiType], ret: AbiType) -> MethodSpec {
    MethodSpec { symbol, recv_abi: RecvAbi::Handle, args, ret }
}

/// Build a Number instance-method spec (receiver = the f64 primitive).
const fn nm(symbol: &'static str, args: &'static [AbiType], ret: AbiType) -> MethodSpec {
    MethodSpec { symbol, recv_abi: RecvAbi::F64, args, ret }
}
