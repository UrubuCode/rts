//! `rts` CLI library. Every command runs through the engine — `rts-codegen` +
//! `rts-cranelift` + `rts-core-rwk`, reached via `rts-host-rwk`.
//!
//! # What left with the old engine
//!
//! Three facades: `abi` (`rts-engine`'s ABI tables), `namespaces`
//! (`rts-runtime`'s), and `rts apis`, which listed the first from the second.
//! Nothing outside this crate imported any of them — the bin re-exported them
//! and no caller followed — and the command had been an error message since the
//! cutover. A facade over a deleted crate is not a smaller version of the
//! feature; it is a name that resolves to nothing.

pub mod errors;
pub mod linker {
    pub use rts_linker::*;
    pub use rts_linker::object_linker;
    pub use rts_linker::system_linker;
    pub use rts_linker::toolchain;
}
pub mod compile_options;
pub mod manifest;
pub mod registers;
pub mod dotenv;
pub mod url_entry;
pub mod cli;

pub use compile_options::{CompilationProfile, CompileOptions, opt_level};
