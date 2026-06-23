//! `rts-adapters` — the runtime-side value model of the new engine, split out of
//! `rts-codegen-new` so it can link into AOT binaries (as a staticlib) the same
//! way `rts-runtime` does, WITHOUT pulling Cranelift into the output.
//!
//! Holds: the NaN-boxed [`value::PolyValue`], hidden-class [`shape`]s, inline-cache
//! [`ic`] cells, data-driven [`dispatch`], the [`repr`] lattice, compile-time
//! [`state`], and every `__rtsadp_*` runtime trampoline (`value::*`). It is
//! Cranelift-free on purpose: the Cranelift *emit* side (box/unbox IR, ABI sig
//! descriptors, call-boundary marshaling) stays in `rts-codegen-new::value`.
pub mod dispatch;
pub mod ic;
pub mod repr;
pub mod shape;
pub mod state;
pub mod value;
