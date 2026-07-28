//! `rts-adapters` — the runtime-side value model of the new engine, split out of
//! `rts-codegen-new` so it can link into AOT binaries (as a staticlib) the same
//! way `rts-runtime` does, WITHOUT pulling Cranelift into the output.
//!
//! Holds: the NaN-boxed [`value::PolyValue`] and every `__rtsadp_*` runtime
//! trampoline (`value::*`), plus [`dispatch`] — the small static method-metadata
//! table some of those trampolines consult at runtime (e.g. the `in` operator's
//! array-proto probe). It is Cranelift-free on purpose: the Cranelift *emit*
//! side (box/unbox IR, ABI sig descriptors, call-boundary marshaling) stays in
//! `rts-codegen-new::value`.
//!
//! The compile-time-only slices of the old value model — the `repr` lattice,
//! hidden-class `shape`s, and `state::reset_codegen_state` — moved into
//! `rts-codegen-new` (they are lowering-time concerns, not AOT-linked runtime
//! surface). `dispatch` stays here because a runtime trampoline
//! (`value::objops::__rtsadp_obj_has`) reads it at runtime, and
//! `rts-codegen-new` already depends on `rts-adapters` — the reverse dependency
//! would be a cycle.
pub mod dispatch;

pub mod value;
