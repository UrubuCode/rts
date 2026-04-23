use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::compile_options::CompileOptions;

pub(crate) const OBJECT_CACHE_SCHEMA: u32 = 9;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ObjectCacheMeta {
    pub(crate) cache_schema: u32,
    pub(crate) source_hash: String,
    /// Hash combinado (SHA256) de todas as dependencias transitivas do modulo.
    /// Invalida o cache quando qualquer modulo importado direta ou indiretamente
    /// mudou — crucial para evitar linkar objects staled contra headers novos.
    #[serde(default)]
    pub(crate) deps_hash: String,
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

pub(crate) fn hash_source(source: &str) -> String {
    let digest = Sha256::digest(source.as_bytes());
    format!("{digest:x}")
}

pub(crate) fn is_cached_object_valid(
    meta_path: &Path,
    object_path: &Path,
    source_hash: &str,
    deps_hash: &str,
    options: &CompileOptions,
    emit_entrypoint: bool,
) -> bool {
    if !object_path.is_file() || !meta_path.is_file() {
        return false;
    }

    let meta = read_object_cache_meta(meta_path).ok().flatten();
    let Some(meta) = meta else {
        return false;
    };

    meta.source_hash == source_hash
        && meta.deps_hash == deps_hash
        && meta.cache_schema == OBJECT_CACHE_SCHEMA
        && meta.profile == options.profile.to_string()
        && meta.debug == options.debug
        && meta.emit_entrypoint == emit_entrypoint
        && meta.rts_version == env!("CARGO_PKG_VERSION")
}

pub(crate) fn write_object_cache_meta(path: &Path, meta: &ObjectCacheMeta) -> Result<()> {
    let cache_value = serde_json::to_value(meta)
        .map_err(|error| anyhow!("failed to encode object cache metadata: {error}"))?;

    let mut root = read_json_object(path)?.unwrap_or_default();
    root.insert("cache".to_string(), cache_value);

    let encoded = serde_json::to_string_pretty(&Value::Object(root))
        .map_err(|error| anyhow!("failed to encode object cache metadata: {error}"))?;
    std::fs::write(path, encoded)
        .with_context(|| format!("failed to write object cache metadata {}", path.display()))
}

fn read_object_cache_meta(path: &Path) -> Result<Option<ObjectCacheMeta>> {
    let Some(root) = read_json_object(path)? else {
        return Ok(None);
    };

    if let Some(cache) = root.get("cache") {
        let parsed = serde_json::from_value::<ObjectCacheMeta>(cache.clone()).map_err(|error| {
            anyhow!(
                "failed to decode cache metadata from {}: {error}",
                path.display()
            )
        })?;
        return Ok(Some(parsed));
    }

    // Backward compatibility for older metadata-only files.
    let parsed = serde_json::from_value::<ObjectCacheMeta>(Value::Object(root)).ok();
    Ok(parsed)
}

fn read_json_object(path: &Path) -> Result<Option<Map<String, Value>>> {
    if !path.is_file() {
        return Ok(None);
    }

    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read object cache metadata {}", path.display()))?;
    let value = serde_json::from_str::<Value>(&raw)
        .map_err(|error| anyhow!("failed to parse {} as JSON: {error}", path.display()))?;

    match value {
        Value::Object(map) => Ok(Some(map)),
        _ => Ok(None),
    }
}
