pub mod cli {
    pub use rts_cli::cli::*;
}
pub mod registers {
    pub use rts_cli::registers::*;
}

pub mod errors {
    pub use rts_cli::errors::*;
}

pub mod crash;
pub(crate) mod runtime_objects;

/// The AOT runtime archive for the host target
/// (`~/.rts/artifacts/<host-triple>.a`), materialized from the copy embedded
/// in this binary.
///
/// It is a fallback rather than the first answer: `rts-cli`'s
/// `runtime_archive` prefers a freshly built `target/` archive, so a
/// developer who just rebuilt `rts-runtime` links against their own build
/// and not against whatever this binary was compiled with.
pub fn rt_artifacts() -> anyhow::Result<std::path::PathBuf> {
    runtime_objects::ensure_artifacts()
}

/// Every `napi_*` symbol this binary carries, by name.
///
/// # Why the bin names this at all
///
/// To make the linker keep them. A `#[unsafe(no_mangle)]` function inside a
/// DEPENDENCY is unreferenced from here, and an rlib nothing touches is an rlib
/// the linker never opens — measured, not assumed: with the crate merely listed
/// as a dependency, `napi_create_double` did not appear in the binary at all.
/// Reading this list is a reference to the module that holds the keep-alive
/// table, which pulls the rest in behind it.
///
/// It is also what the export arguments will be built from when P8b lands, so
/// the same list serves both and there is no second one to drift.
///
/// Present is NOT exported: a `.node` looks these up in the process's export
/// table, and this build passes no `/EXPORT:` or `--export-dynamic`. See
/// `crates/rts-napi/PLAN.md`.
pub fn napi_symbols() -> &'static [&'static str] {
    rts_napi::exported::NAMES
}
