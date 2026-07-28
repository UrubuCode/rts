pub mod cli {
    pub use rts_cli::cli::*;
}
pub mod diagnostics;
pub mod registers {
    pub use rts_cli::registers::*;
}

pub mod crash;
pub(crate) mod runtime_objects;

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
