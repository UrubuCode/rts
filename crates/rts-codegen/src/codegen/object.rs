use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ObjectArtifact {
    pub path: PathBuf,
    pub bytes_written: usize,
    pub emitted_calls: usize,
    pub used_namespaces: HashSet<String>,
}
