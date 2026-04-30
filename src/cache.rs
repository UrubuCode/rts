use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const CACHE_SUBDIR: &str = "node_modules/.rts";
const RTS_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize, Deserialize)]
struct ObjMeta {
    source_checksum: String,
    rts_version: String,
    target: String,
    #[serde(default)]
    compiler_fingerprint: String,
    used_namespaces: Vec<String>,
}

pub struct CacheHit {
    pub obj_path: PathBuf,
    pub used_namespaces: HashSet<String>,
}

pub struct ObjCache {
    root: PathBuf,
}

impl ObjCache {
    pub fn for_input(input: &Path) -> Self {
        // RTS_CACHE_DIR override absoluto: util pra projetos sem
        // node_modules/ (workspaces Rust, etc) ou pra isolar caches em CI.
        if let Ok(custom) = std::env::var("RTS_CACHE_DIR") {
            let p = PathBuf::from(custom);
            if !p.as_os_str().is_empty() {
                return Self { root: p };
            }
        }
        Self {
            root: find_project_root(input).join(CACHE_SUBDIR),
        }
    }

    pub fn lookup(&self, source: &Path) -> Result<Option<CacheHit>> {
        let checksum = file_sha256(source)?;
        let target = crate::linker::toolchain::TargetTriple::resolve(None).triple;
        let compiler_fingerprint = compiler_fingerprint();
        let dir = self.obj_dir(&checksum);
        let obj_path = dir.join("output.o");
        let meta_path = dir.join("output.ometa");

        if !obj_path.is_file() || !meta_path.is_file() {
            return Ok(None);
        }

        let meta: ObjMeta = serde_json::from_slice(
            &std::fs::read(&meta_path)
                .with_context(|| format!("failed to read {}", meta_path.display()))?,
        )
        .with_context(|| format!("malformed ometa at {}", meta_path.display()))?;

        if meta.source_checksum != checksum
            || meta.rts_version != RTS_VERSION
            || meta.target != target
            || meta.compiler_fingerprint != compiler_fingerprint
        {
            return Ok(None);
        }

        Ok(Some(CacheHit {
            obj_path,
            used_namespaces: meta.used_namespaces.into_iter().collect(),
        }))
    }

    pub fn store(
        &self,
        source: &Path,
        compiled_obj: &Path,
        used_namespaces: &HashSet<String>,
    ) -> Result<PathBuf> {
        let checksum = file_sha256(source)?;
        let dir = self.obj_dir(&checksum);
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create cache dir {}", dir.display()))?;

        let dest = dir.join("output.o");
        std::fs::copy(compiled_obj, &dest)
            .with_context(|| format!("failed to copy object to cache {}", dest.display()))?;

        let mut sorted_ns: Vec<String> = used_namespaces.iter().cloned().collect();
        sorted_ns.sort();

        let target = crate::linker::toolchain::TargetTriple::resolve(None).triple;
        let meta = ObjMeta {
            source_checksum: checksum,
            rts_version: RTS_VERSION.to_string(),
            target,
            compiler_fingerprint: compiler_fingerprint(),
            used_namespaces: sorted_ns,
        };

        let meta_path = dir.join("output.ometa");
        std::fs::write(
            &meta_path,
            serde_json::to_vec_pretty(&meta).context("failed to serialize ometa")?,
        )
        .with_context(|| format!("failed to write {}", meta_path.display()))?;

        Ok(dest)
    }

    fn obj_dir(&self, checksum: &str) -> PathBuf {
        self.root.join("obj").join(checksum)
    }
}


fn file_sha256(path: &Path) -> Result<String> {
    let bytes =
        std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

fn compiler_fingerprint() -> String {
    let Ok(exe) = std::env::current_exe() else {
        return format!("rts-{RTS_VERSION}");
    };
    let Ok(bytes) = std::fs::read(exe) else {
        return format!("rts-{RTS_VERSION}");
    };
    format!("{:x}", Sha256::digest(&bytes))
}

fn find_project_root(input: &Path) -> PathBuf {
    let start = input
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let mut dir = start.clone();
    loop {
        if dir.join("package.json").exists() || dir.join("node_modules").is_dir() {
            return dir;
        }
        match dir.parent() {
            Some(p) => dir = p.to_path_buf(),
            None => return start,
        }
    }
}
