use std::path::PathBuf;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

/// Combined Rust staticlib for all runtime namespaces (gc + io + fs + …).
/// Compiled at build time; includes all Rust std dependencies needed at link.
pub(crate) static RUNTIME_ARCHIVE: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/runtime_support.a"));

/// Returns `~/.rts/artifacts.a`, extracting or updating it when the embedded
/// archive differs from what's on disk (SHA-256 comparison).
///
/// This is a global user-level cache: all projects share the same file.
/// Re-extraction only happens when `rts` itself is rebuilt with a new runtime.
pub(crate) fn ensure_artifacts() -> Result<PathBuf> {
    let path = artifacts_path()?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create ~/.rts dir {}", parent.display()))?;
    }

    let embedded_hash = sha256_hex(RUNTIME_ARCHIVE);

    let needs_update = if path.is_file() {
        let on_disk = std::fs::read(&path)
            .with_context(|| format!("read {}", path.display()))?;
        sha256_hex(&on_disk) != embedded_hash
    } else {
        true
    };

    if needs_update {
        std::fs::write(&path, RUNTIME_ARCHIVE)
            .with_context(|| format!("write {}", path.display()))?;
    }

    Ok(path)
}

fn artifacts_path() -> Result<PathBuf> {
    Ok(crate::registers::rts_home()?.join("artifacts.a"))
}

fn sha256_hex(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
