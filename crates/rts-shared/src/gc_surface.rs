//! Re-export of the collector/runtime `extern "C"` symbols the universal
//! layer (`rts-shared`) calls — see `rts_engine::gc_surface` for the
//! canonical declarations and the layering rationale (single declaration
//! site, resolved by link; real bodies live above `rts-engine`, in
//! `rts-std`/`rts-primitives`).
pub use rts_engine::gc_surface::*;
