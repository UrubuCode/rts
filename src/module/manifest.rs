use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

use crate::compile_options::CompileOptions;

use super::attach_trace;

pub(crate) type ManifestCache = BTreeMap<PathBuf, PackageManifest>;

#[derive(Debug, Clone)]
pub(crate) struct PackageManifest {
    pub(crate) manifest_path: PathBuf,
    pub(crate) package_dir: PathBuf,
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) main: String,
    pub(crate) dependencies: BTreeMap<String, DependencySpec>,
}

#[derive(Debug, Clone)]
pub(crate) enum DependencySpec {
    Npm { version: String },
    Url { url: String },
    LocalPath { path: String },
}

#[derive(Debug, Deserialize)]
pub(super) struct RawPackageManifest {
    pub(super) name: Option<String>,
    pub(super) version: Option<String>,
    pub(super) main: Option<String>,
    #[serde(default)]
    pub(super) dependencies: BTreeMap<String, String>,
}

pub(crate) fn find_owner_manifest(
    module_path: &Path,
    workspace_root: &Path,
    manifest_cache: &mut ManifestCache,
    options: CompileOptions,
    trace_route: &[String],
) -> Result<Option<PackageManifest>> {
    let workspace_root = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf());

    let mut current = module_path.parent();
    while let Some(dir) = current {
        let manifest_path = dir.join("package.json");
        if manifest_path.exists() {
            return load_package_manifest(&manifest_path, manifest_cache)
                .map(Some)
                .with_context(|| {
                    attach_trace(
                        format!("invalid package manifest {}", manifest_path.display()),
                        trace_route,
                        options,
                    )
                });
        }

        if dir == workspace_root {
            break;
        }

        current = dir.parent();
    }

    Ok(None)
}

pub(crate) fn load_package_manifest(
    path: &Path,
    cache: &mut ManifestCache,
) -> Result<PackageManifest> {
    let package_dir = path
        .parent()
        .ok_or_else(|| {
            anyhow!(
                "package manifest has no parent directory: {}",
                path.display()
            )
        })?
        .canonicalize()
        .with_context(|| {
            format!(
                "failed to canonicalize package directory {}",
                path.display()
            )
        })?;

    if let Some(cached) = cache.get(&package_dir) {
        return Ok(cached.clone());
    }

    let raw_content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read package manifest {}", path.display()))?;
    let clean_content = strip_json_comments(&raw_content);
    let raw: RawPackageManifest = serde_json::from_str(&clean_content)
        .with_context(|| format!("failed to parse package manifest {}", path.display()))?;

    let default_name = package_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("package")
        .to_string();

    let mut dependencies = BTreeMap::new();
    for (name, spec) in raw.dependencies {
        dependencies.insert(name, parse_dependency_spec(&spec));
    }

    let manifest = PackageManifest {
        manifest_path: path.canonicalize().unwrap_or_else(|_| path.to_path_buf()),
        package_dir: package_dir.clone(),
        name: raw.name.unwrap_or(default_name),
        version: raw.version.unwrap_or_else(|| "0.0.0".to_string()),
        main: raw.main.unwrap_or_else(|| "main.ts".to_string()),
        dependencies,
    };

    cache.insert(package_dir, manifest.clone());
    Ok(manifest)
}

pub(crate) fn parse_dependency_spec(raw: &str) -> DependencySpec {
    let value = raw.trim();

    if let Some(version) = value.strip_prefix("npm:") {
        let version = version.trim();
        return DependencySpec::Npm {
            version: if version.is_empty() {
                "latest".to_string()
            } else {
                version.to_string()
            },
        };
    }

    if is_remote_url(value) {
        return DependencySpec::Url {
            url: value.to_string(),
        };
    }

    if value.starts_with("./") || value.starts_with("../") || value.starts_with('/') {
        return DependencySpec::LocalPath {
            path: value.to_string(),
        };
    }

    DependencySpec::Npm {
        version: if value.is_empty() {
            "latest".to_string()
        } else {
            value.to_string()
        },
    }
}

pub(crate) fn strip_json_comments(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_string {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            output.push(ch);
            continue;
        }

        if ch == '/' && matches!(chars.peek(), Some('/')) {
            let _ = chars.next();
            for next in chars.by_ref() {
                if next == '\n' {
                    output.push('\n');
                    break;
                }
            }
            continue;
        }

        output.push(ch);
    }

    output
}

fn is_remote_url(specifier: &str) -> bool {
    specifier.starts_with("http://") || specifier.starts_with("https://")
}
