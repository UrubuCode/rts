//! Produced object file artifact returned by the codegen entry point.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ObjectArtifact {
    pub path: PathBuf,
    pub bytes_written: usize,
    pub emitted_calls: usize,
}
