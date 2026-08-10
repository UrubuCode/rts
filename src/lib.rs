pub mod cli {
    pub use rts_cli::cli::*;
}
pub mod diagnostics;
pub mod registers {
    pub use rts_cli::registers::*;
}

pub mod crash;
pub(crate) mod runtime_objects;

/// The AOT runtime archive for the host target
/// (`~/.rts/artifacts-rwk/<host-triple>.a`), materialized from the copy embedded
/// in this binary.
///
/// It is a fallback rather than the first answer: `rts-cli`'s
/// `runtime_archive_rwk` prefers a freshly built `target/` archive, so a
/// developer who just rebuilt `rts-runtime-rwk` links against their own build
/// and not against whatever this binary was compiled with.
pub fn rt_artifacts_rwk() -> anyhow::Result<std::path::PathBuf> {
    runtime_objects::ensure_artifacts_rwk()
}
