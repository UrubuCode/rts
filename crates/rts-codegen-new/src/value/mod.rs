//! `value` facade — the runtime-side value model (`PolyValue`, the bit-layout
//! constants, and every `__rtsadp_*` trampoline) lives in `rts_runtime::adapters`
//! (folded in from the former Cranelift-free `rts-adapters` crate) so it links
//! into AOT binaries as part of the `rts-runtime` staticlib. This module
//! RE-EXPORTS it unchanged — every
//! existing `crate::value::{PolyValue, encode, BOX_BASE, objops, genops, …}`
//! path keeps resolving — and additionally hosts the Cranelift *emit* side that
//! belongs to the codegen front-end and must NOT cross into the runtime crate:
//! `emit` (box/unbox IR), `abi_sig` (real-symbol signatures), `emit_marshal`
//! (call-boundary marshaling), and the JIT-roundtrip `tests`.
pub use rts_runtime::adapters::value::*;

// Cranelift emit side — stays in the codegen crate (pulls cranelift_frontend).
pub mod abi_sig;
mod emit;
pub mod emit_marshal;
#[cfg(test)]
mod tests;

// Re-export the Cranelift emit helpers so existing call sites
// (`crate::value::emit_box_int32`, …) keep resolving unchanged.
pub use emit::{
    emit_box_double, emit_box_int32, emit_is_boxed, emit_is_double, emit_tagged_number_to_f64,
    emit_unbox_double, emit_unbox_int32,
};
