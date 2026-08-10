//! The AOT runtime archive `rts compile` links against, embedded in this binary
//! and materialized under `~/.rts/` on demand.
//!
//! # What used to be here
//!
//! Two archives. The old engine's `rts-runtime` staticlib was embedded beside
//! this one, extracted to `~/.rts/artifacts/`, and reachable through
//! `rts::rt_artifacts` / `rts-cli`'s `runtime_archive` — none of which anything
//! called after `rts compile` moved to the new engine. It was ~18 MB of the
//! shipped binary and a `build.rs` step compressing ~99 MB at level 19 on every
//! build that touched the runtime, for a path with no callers.
//!
//! The cross-target machinery went with it: a prebuilt-directory lookup, a
//! download URL and two environment variables, all reachable only from the old
//! engine's `ensure_artifacts_for_target`. Nothing here asks for a cross-target
//! archive yet, and re-deriving that machinery when something does is cheaper
//! than keeping a copy nothing exercises.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

/// Target triple the embedded archive was compiled for (the build host).
///
/// Emitted by `build.rs` from Cargo's `TARGET`. Cross targets are not embedded.
const HOST_TARGET: &str = env!("RTS_HOST_TARGET");

/// The new engine's AOT runtime archive (`rts-runtime-rwk`, over `rts-core-rwk`
/// + `rts-std-rwk` + `rts-node-rwk`), zstd-compressed at build time.
static RUNTIME_ARCHIVE_RWK_ZST: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/runtime_support_rwk.a.zst"));

/// sha256 (hex) of the decompressed archive, or `PLACEHOLDER` if
/// `rts-runtime-rwk` had no staticlib built at compile time.
///
/// The hash is what makes re-extraction conditional: an `rts` rebuilt with a new
/// runtime writes a new archive, and one that was not rebuilt reads the file
/// already on disk.
static RUNTIME_ARCHIVE_RWK_SHA: &str =
    include_str!(concat!(env!("OUT_DIR"), "/runtime_support_rwk.sha256"));

/// Returns the runtime archive for the host target, materializing it on demand
/// from the embedded copy.
pub(crate) fn ensure_artifacts_rwk() -> Result<PathBuf> {
    let path = artifact_path_rwk(HOST_TARGET)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create artifacts-rwk dir {}", parent.display()))?;
    }

    let expected = RUNTIME_ARCHIVE_RWK_SHA.trim();
    if expected == "PLACEHOLDER" {
        bail!(
            "the new engine's embedded runtime archive is a placeholder — \
             rts-runtime-rwk had no staticlib built when this `rts` was compiled. \
             `cargo build -p rts-runtime-rwk` (matching profile) then rebuild `rts`."
        );
    }

    let up_to_date = if path.is_file() {
        let on_disk = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
        sha256_hex(&on_disk) == expected
    } else {
        false
    };

    if !up_to_date {
        let bytes =
            zstd::decode_all(RUNTIME_ARCHIVE_RWK_ZST).context("decompress runtime archive")?;
        std::fs::write(&path, &bytes).with_context(|| format!("write {}", path.display()))?;
    }

    Ok(path)
}

/// `~/.rts/artifacts-rwk/<target>.a`.
///
/// The `-rwk` suffix outlived the directory it was distinguishing itself from —
/// the old engine's `~/.rts/artifacts/` is no longer written by anything. Kept
/// because an installed `rts` already extracted into this path, and renaming it
/// would leave every existing installation with a stale archive it never reads.
fn artifact_path_rwk(target: &str) -> Result<PathBuf> {
    Ok(crate::registers::rts_home()?
        .join("artifacts-rwk")
        .join(format!("{target}.a")))
}

fn sha256_hex(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
