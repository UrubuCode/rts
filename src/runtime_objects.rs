use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

/// Combined Rust staticlib for all runtime namespaces (gc + io + fs).
/// Compiled at build time; includes all Rust std dependencies needed at link.
pub(crate) static RUNTIME_ARCHIVE: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/runtime_support.a"));

/// Extracts the runtime archive into `cache_dir` if not already present.
///
/// Returns the path to the extracted `.a` file. The linker accepts archives
/// directly; dead code elimination (`--gc-sections` / `/OPT:REF`) strips
/// unused namespace functions from the final binary.
pub(crate) fn extract_runtime_archive(cache_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(cache_dir)
        .with_context(|| format!("failed to create runtime cache {}", cache_dir.display()))?;

    let hash = format!("{:x}", Sha256::digest(RUNTIME_ARCHIVE));
    let archive_path = cache_dir.join(format!("runtime_support_{}.a", &hash[..16]));

    if !archive_path.is_file() {
        std::fs::write(&archive_path, RUNTIME_ARCHIVE).with_context(|| {
            format!("failed to write runtime archive {}", archive_path.display())
        })?;
    }

    Ok(archive_path)
}
