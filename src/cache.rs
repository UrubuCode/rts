use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::codegen;
use crate::compile_options::CompileOptions;

pub(crate) const OBJECT_CACHE_SCHEMA: u32 = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ObjectCacheMeta {
    pub(crate) cache_schema: u32,
    pub(crate) source_hash: String,
    pub(crate) profile: String,
    pub(crate) debug: bool,
    pub(crate) emit_entrypoint: bool,
    pub(crate) object_bytes: u64,
    pub(crate) rts_version: String,
}

#[derive(Debug, Default)]
pub(crate) struct RuntimeObjectArtifacts {
    pub(crate) object_paths: Vec<PathBuf>,
    pub(crate) bytes_written: usize,
    pub(crate) cache_hits: usize,
    pub(crate) cache_misses: usize,
}

#[derive(Debug)]
pub(crate) struct CachedObjectEmission {
    pub(crate) path: PathBuf,
    pub(crate) bytes_written: usize,
    pub(crate) cache_hit: bool,
}

pub(crate) fn hash_source(source: &str) -> String {
    let digest = Sha256::digest(source.as_bytes());
    format!("{digest:x}")
}

pub(crate) fn is_cached_object_valid(
    meta_path: &Path,
    object_path: &Path,
    source_hash: &str,
    options: &CompileOptions,
    emit_entrypoint: bool,
) -> bool {
    if !object_path.is_file() || !meta_path.is_file() {
        return false;
    }

    let meta = std::fs::read_to_string(meta_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<ObjectCacheMeta>(&raw).ok());
    let Some(meta) = meta else {
        return false;
    };

    meta.source_hash == source_hash
        && meta.cache_schema == OBJECT_CACHE_SCHEMA
        && meta.profile == options.profile.to_string()
        && meta.debug == options.debug
        && meta.emit_entrypoint == emit_entrypoint
        && meta.rts_version == env!("CARGO_PKG_VERSION")
}

pub(crate) fn write_object_cache_meta(path: &Path, meta: &ObjectCacheMeta) -> Result<()> {
    let encoded = serde_json::to_string_pretty(meta)
        .map_err(|error| anyhow!("failed to encode object cache metadata: {error}"))?;
    std::fs::write(path, encoded)
        .with_context(|| format!("failed to write object cache metadata {}", path.display()))
}

pub(crate) fn emit_cached_object_bytes<F>(
    deps_dir: &Path,
    stem: &str,
    source_hash: &str,
    options: &CompileOptions,
    emit_entrypoint: bool,
    build: F,
) -> Result<CachedObjectEmission>
where
    F: FnOnce() -> Result<Vec<u8>>,
{
    let object_path = deps_dir.join(format!("{stem}.o"));
    let meta_path = deps_dir.join(format!("{stem}.m"));

    if is_cached_object_valid(
        &meta_path,
        &object_path,
        source_hash,
        options,
        emit_entrypoint,
    ) {
        let bytes_written = std::fs::metadata(&object_path)
            .map(|metadata| metadata.len() as usize)
            .unwrap_or(0);
        return Ok(CachedObjectEmission {
            path: object_path,
            bytes_written,
            cache_hit: true,
        });
    }

    let bytes = build()?;
    let artifact = codegen::object::write_object_file(&object_path, &bytes)?;
    write_object_cache_meta(
        &meta_path,
        &ObjectCacheMeta {
            cache_schema: OBJECT_CACHE_SCHEMA,
            source_hash: source_hash.to_string(),
            profile: options.profile.to_string(),
            debug: options.debug,
            emit_entrypoint,
            object_bytes: artifact.bytes_written as u64,
            rts_version: env!("CARGO_PKG_VERSION").to_string(),
        },
    )?;

    Ok(CachedObjectEmission {
        path: artifact.path,
        bytes_written: artifact.bytes_written,
        cache_hit: false,
    })
}
