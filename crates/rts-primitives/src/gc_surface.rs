//! Re-export of the collector/runtime `extern "C"` symbols the primordial
//! classes (`rts-primitives`) call — see `rts_engine::gc_surface` for the
//! canonical declarations and the layering rationale (single declaration
//! site, resolved by link; real bodies live above `rts-engine`, in
//! `rts-std`).
pub use rts_engine::gc_surface::*;
