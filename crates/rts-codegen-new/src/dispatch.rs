//! Data-driven method/builtin dispatch — the "no builtins in the engine" rule.
//!
//! The engine may name ONLY primordial classes (String/Object/Array/Function/
//! Promise/Boolean/Number/Error+subclasses). EVERYTHING else — Map/Set/Date/
//! RegExp/console/JSON/Math/... — resolves through the Registry (`SPECS` /
//! `GLOBAL_CLASS_SPECS`) as `MethodSpec` metadata, via ONE generic emit path.
//! This deletes the old `calls/mod.rs` 4.6k-LOC switchboard (5x-duplicated
//! `JSON.stringify`, twice-duplicated `Math.max`, hardcoded `console.*` lists).
//!
//! Dispatch on a `PolyValue`: read the tag -> heap kind -> the kind's method
//! table (primordial: direct; registered: Registry lookup) -> emit. Intrinsics
//! (sqrt/abs/min/max) still inline as Cranelift IR when the spec marks them.

/// Resolved call target for `recv.method(args)`.
pub enum Target {
    /// Inline this as native Cranelift IR (intrinsic on the spec).
    Intrinsic(&'static str),
    /// Emit a typed `call` to this extern symbol (the generic path).
    Extern(&'static str),
    /// Dispatch through a shape/IC (user object method).
    ShapeMethod,
}

/// Resolve a method call to a `Target` using ONLY registry metadata + the
/// primordial set. No per-method special cases.
pub fn resolve_method(_recv_kind: &str, _method: &str, _argc: usize) -> Option<Target> {
    todo!("phase: dispatch — drive entirely from SPECS/GLOBAL_CLASS_SPECS metadata")
}
