//! ABI: the stable vocabulary between codegen and runtime (types, symbols,
//! signatures, handles, guards) — re-exported verbatim from the standalone
//! `rts-abi` crate.
//!
//! Present in BOTH `rts-natives` and `rts-engine` on purpose, and it costs
//! nothing: both are `pub use rts_abi::*`, so the two paths name the same types
//! from the same crate. `rts-natives` needs its own because the `#[rtse::*]`
//! macros emit `::rts_engine::abi::SymbolDesc`, and the `extern crate self as
//! rts_engine` shim in `lib.rs` points that at this module.
//!
//! Contract-only, by design: `rts-abi` holds `AbiType`, `SymbolDesc` and the
//! symbol-naming rule, and not a single `extern "C"` function — which is what
//! lets `rts-macro` (declare) and `rts-symbol-baker` (link) derive the same
//! symbol from one place. See `docs/engine/architecture.md`.

pub use rts_abi::*;
