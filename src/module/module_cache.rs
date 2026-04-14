use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::import_resolver::resolve_source_candidate;
use super::manifest::{RawPackageManifest, strip_json_comments};

pub(super) const MODULES_ENV_VAR: &str = "RTS_MODULES_PATH";

#[derive(Debug, Clone)]
pub(crate) struct ModuleCache {
    base_dir: PathBuf,
}

impl ModuleCache {
    pub(crate) fn discover() -> Result<Self> {
        let base_dir = resolve_modules_base_dir()?;
        std::fs::create_dir_all(&base_dir).with_context(|| {
            format!(
                "failed to create RTS module cache directory {}",
                base_dir.display()
            )
        })?;
        Ok(Self { base_dir })
    }

    pub(crate) fn resolve_cached_npm_dependency(
        &self,
        module_name: &str,
        version: &str,
    ) -> Result<PathBuf> {
        let version = sanitize_segment(version);
        let module_name = sanitize_segment(module_name);
        let root = self.base_dir.join("npm").join(module_name).join(version);

        if !root.exists() {
            bail!(
                "cached npm module not found at {} (expected RTS_MODULES_PATH layout node_modules/.rts/modules/npm/<name>/<version>/...)",
                root.display()
            );
        }

        resolve_cached_module_entry(&root)
    }

    pub(crate) fn fetch_remote_import(&self, alias: Option<&str>, url: &str) -> Result<PathBuf> {
        let name = alias
            .map(sanitize_segment)
            .unwrap_or_else(|| sanitize_segment(&url_module_name(url)));
        let version = format!("{:016x}", fnv1a64(url.as_bytes()));
        let root = self.base_dir.join("url").join(name).join(version);
        let entry = root.join("main.ts");

        if !entry.exists() {
            std::fs::create_dir_all(&root)
                .with_context(|| format!("failed to create cache directory {}", root.display()))?;
            let body = download_remote_module(url)?;
            std::fs::write(&entry, body).with_context(|| {
                format!("failed to write remote module cache {}", entry.display())
            })?;
        }

        resolve_source_candidate(&entry)
    }
}

fn resolve_cached_module_entry(root: &Path) -> Result<PathBuf> {
    let manifest_path = root.join("package.json");
    if manifest_path.exists() {
        let raw_content = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?;
        let clean_content = strip_json_comments(&raw_content);
        if let Ok(raw) = serde_json::from_str::<RawPackageManifest>(&clean_content) {
            if let Some(main) = raw.main {
                let candidate = root.join(main);
                if candidate.exists() {
                    return resolve_source_candidate(&candidate);
                }
            }
        }
    }

    for candidate in [
        root.join("main.ts"),
        root.join("main.js"),
        root.join("index.ts"),
        root.join("index.js"),
        root.join("mod.ts"),
    ] {
        if candidate.exists() {
            return resolve_source_candidate(&candidate);
        }
    }

    bail!("cached module entry not found in {}", root.display())
}

fn resolve_modules_base_dir() -> Result<PathBuf> {
    if let Ok(configured) = std::env::var(MODULES_ENV_VAR) {
        let configured = configured.trim();
        if configured.is_empty() || configured == "~" {
            return default_modules_base_dir();
        }

        let expanded = expand_tilde_path(configured)?;
        return Ok(expanded);
    }

    default_modules_base_dir()
}

fn default_modules_base_dir() -> Result<PathBuf> {
    Ok(PathBuf::from("node_modules").join(".rts").join("modules"))
}

fn home_dir() -> Result<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return Ok(PathBuf::from(home));
        }
    }

    if let Ok(profile) = std::env::var("USERPROFILE") {
        if !profile.trim().is_empty() {
            return Ok(PathBuf::from(profile));
        }
    }

    bail!("unable to resolve user home directory for RTS module cache")
}

fn expand_tilde_path(raw: &str) -> Result<PathBuf> {
    if raw == "~" {
        return default_modules_base_dir();
    }

    if let Some(rest) = raw.strip_prefix("~/") {
        return home_dir().map(|home| home.join(rest));
    }

    Ok(PathBuf::from(raw))
}

fn sanitize_segment(raw: &str) -> String {
    let sanitized = raw
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();

    if sanitized.is_empty() {
        "module".to_string()
    } else {
        sanitized
    }
}

fn url_module_name(url: &str) -> String {
    let without_query = url
        .split_once('?')
        .map(|(head, _)| head)
        .unwrap_or(url)
        .split_once('#')
        .map(|(head, _)| head)
        .unwrap_or(url);

    let raw_name = without_query.rsplit('/').next().unwrap_or("remote").trim();

    let name = raw_name
        .strip_suffix(".ts")
        .or_else(|| raw_name.strip_suffix(".rts"))
        .unwrap_or(raw_name);

    if name.is_empty() {
        "remote".to_string()
    } else {
        name.to_string()
    }
}

fn download_remote_module(url: &str) -> Result<String> {
    match ureq::get(url).call() {
        Ok(response) => response
            .into_string()
            .with_context(|| format!("failed to decode remote module body from {}", url)),
        Err(ureq::Error::Status(code, response)) => bail!(
            "failed to fetch remote module {} (HTTP {} {})",
            url,
            code,
            response.status_text()
        ),
        Err(ureq::Error::Transport(error)) => {
            bail!("failed to fetch remote module {} ({})", url, error)
        }
    }
}

fn fnv1a64(input: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in input {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}
