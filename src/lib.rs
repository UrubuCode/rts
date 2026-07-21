pub mod cli {
    pub use rts_cli::cli::*;
}
pub mod diagnostics;
pub mod registers {
    pub use rts_cli::registers::*;
}

pub mod crash;
pub(crate) mod runtime_objects;

/// Step 10, slice 2 — the baked resident prelude compiled into this binary.
///
/// `build.rs` writes both files into `OUT_DIR`: either the real generated table +
/// manifest (when `RTS_PRELUDE_DIR` points at a baker output and `prelude.o` is
/// linked in) or an inert stub (`prelude_symbols()` → empty, empty manifest). The
/// `include!`d `prelude_symbols()` takes the address of every resident prelude
/// symbol, so the linker keeps them; `main` installs the table + manifest into the
/// engine. With the stub, `prelude_symbols()` is empty and nothing is installed.
pub mod prelude_baked {
    include!(concat!(env!("OUT_DIR"), "/prelude_symbols.rs"));

    /// The bincode-serialized `PreludeManifest` (empty in the stub build).
    pub const MANIFEST: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/prelude_manifest.bin"));
}

/// Runtime archive for the host target (`~/.rts/artifacts/<host-triple>.a`).
pub fn rt_artifacts() -> anyhow::Result<std::path::PathBuf> {
    runtime_objects::ensure_artifacts()
}

/// Runtime archive for an explicit target triple
/// (`~/.rts/artifacts/<target>.a`). The host archive is embedded; cross targets
/// are resolved from prebuilt per-target archives.
pub fn rt_artifacts_for_target(target: &str) -> anyhow::Result<std::path::PathBuf> {
    runtime_objects::ensure_artifacts_for_target(target)
}
