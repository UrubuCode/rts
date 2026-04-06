pub mod object_linker;

use std::path::{Path, PathBuf};

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct LinkedBinary {
    pub path: PathBuf,
    pub backend: String,
    pub format: String,
}

pub fn link_object_to_binary(object_path: &Path, output_path: &Path) -> Result<LinkedBinary> {
    let artifact = object_linker::link(object_path, output_path)?;

    Ok(LinkedBinary {
        path: artifact.path,
        backend: "object".to_string(),
        format: artifact.format,
    })
}
