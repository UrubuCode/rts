pub mod cli {
    pub use rts_cli::cli::*;
}
pub mod diagnostics;
pub mod registers {
    pub use rts_cli::registers::*;
}

pub mod crash;
pub(crate) mod runtime_objects;

/// Step 10, slice 2 — the baked resident-prelude MANIFEST embedded in this binary.
///
/// `build.rs` writes `OUT_DIR/prelude_manifest.bin`: either the real baked manifest
/// (when `RTS_PRELUDE_DIR` points at a baker output) or empty (the default). `main`
/// installs it into the engine, which replays the prelude machine code via
/// `define_function_bytes`. Empty → nothing installed → the run path uses the
/// fallback.
pub mod prelude_baked {
    /// The bincode-serialized `PreludeManifest` (empty in the default build).
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
