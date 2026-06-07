use std::path::PathBuf;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

/// zstd-compressed combined Rust staticlib for all runtime namespaces
/// (gc + io + fs + …). Compiled + compressed at build time; decompressed to
/// `~/.rts/artifacts.a` on demand. Keeps the shipped `rts` binary small
/// (~99MB raw -> ~18MB embedded).
static RUNTIME_ARCHIVE_ZST: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/runtime_support.a.zst"));

/// sha256 (hex) of the *decompressed* archive, computed by `build.rs`.
/// The sentinel `PLACEHOLDER` means the rts-runtime staticlib was not available
/// at compile time (AOT will bail with a clear message; JIT is unaffected).
static RUNTIME_ARCHIVE_SHA: &str =
    include_str!(concat!(env!("OUT_DIR"), "/runtime_support.sha256"));

/// Returns `~/.rts/artifacts.a`, extracting (decompressing) the embedded archive
/// when the file on disk is missing or differs from the embedded one.
///
/// This is a global user-level cache: all projects share the same file.
/// Re-extraction only happens when `rts` itself is rebuilt with a new runtime.
/// Decompression is skipped entirely when the on-disk file already matches the
/// embedded sha256 (the common case after the first run).
pub(crate) fn ensure_artifacts() -> Result<PathBuf> {
    let expected = RUNTIME_ARCHIVE_SHA.trim();
    if expected == "PLACEHOLDER" {
        anyhow::bail!(
            "runtime archive is a placeholder — it was not built when `rts` was \
             compiled. AOT (`rts compile`) needs the full runtime staticlib. \
             Build it with `cargo build -p rts-runtime` (matching the profile), \
             then rebuild `rts`. JIT (`rts run`) does not require this."
        );
    }
    let path = artifacts_path()?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create ~/.rts dir {}", parent.display()))?;
    }

    let up_to_date = if path.is_file() {
        let on_disk = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
        sha256_hex(&on_disk) == expected
    } else {
        false
    };

    if !up_to_date {
        let bytes = decompress(RUNTIME_ARCHIVE_ZST)?;
        std::fs::write(&path, &bytes).with_context(|| format!("write {}", path.display()))?;
    }

    Ok(path)
}

fn decompress(zst: &[u8]) -> Result<Vec<u8>> {
    zstd::decode_all(zst).context("decompress embedded runtime archive")
}

fn artifacts_path() -> Result<PathBuf> {
    Ok(crate::registers::rts_home()?.join("artifacts.a"))
}

fn sha256_hex(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
