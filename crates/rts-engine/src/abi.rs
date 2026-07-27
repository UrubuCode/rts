//! ABI: the stable vocabulary between codegen and runtime (types, symbols,
//! signatures, handles, guards). Extracted into the standalone `rts-abi`
//! crate so anything that only wants to NAME a symbol can depend on the
//! contract without depending on this crate's heap/GC/Registry
//! implementation (see `docs/specs/rts-macro-single-source.md`). Re-exported
//! here verbatim so every existing `rts_engine::abi::X` import path keeps
//! compiling unchanged.

pub use rts_abi::*;
