use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct ObjectArtifact {
    pub path: PathBuf,
    pub bytes_written: usize,
}

pub fn write_object_file(path: &Path, bytes: &[u8]) -> Result<ObjectArtifact> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    std::fs::write(path, bytes)
        .with_context(|| format!("failed to write object file {}", path.display()))?;

    Ok(ObjectArtifact {
        path: path.to_path_buf(),
        bytes_written: bytes.len(),
    })
}
