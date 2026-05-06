use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

use super::register_dir;

#[derive(Debug, Deserialize)]
struct NpmVersionMeta {
    version: String,
    dist: NpmDist,
    #[serde(default)]
    dependencies: BTreeMap<String, String>,
    #[serde(default)]
    bin: NpmBin,
    main: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NpmDist {
    tarball: String,
    integrity: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(untagged)]
enum NpmBin {
    #[default]
    None,
    Single(String),
    Map(BTreeMap<String, String>),
}

impl NpmBin {
    fn into_map(self, pkg_name: &str) -> BTreeMap<String, String> {
        match self {
            NpmBin::None => BTreeMap::new(),
            NpmBin::Single(path) => {
                let mut m = BTreeMap::new();
                // Use the last segment after / or @ as the bin name
                let name = pkg_name.rsplit('/').next().unwrap_or(pkg_name);
                m.insert(name.to_string(), path);
                m
            }
            NpmBin::Map(m) => m,
        }
    }
}

#[derive(Debug, Deserialize)]
struct NpmFullManifest {
    name: String,
    #[serde(rename = "dist-tags", default)]
    dist_tags: BTreeMap<String, String>,
    versions: BTreeMap<String, NpmVersionMeta>,
}

#[derive(Debug, Clone)]
pub struct ResolvedPackage {
    pub name: String,
    pub version: String,
    pub tarball_url: String,
    pub integrity: Option<String>,
    pub dependencies: BTreeMap<String, String>,
    pub bin: BTreeMap<String, String>,
    pub main: Option<String>,
}

/// Resolve version spec and fetch to `~/.rts/register/npm/<name>/<version>/`.
/// Returns `(resolved_pkg, register_path)`. Does nothing if already cached.
pub fn resolve_and_fetch(name: &str, version_spec: &str) -> Result<(ResolvedPackage, PathBuf)> {
    let pkg = resolve_version(name, version_spec.trim())?;
    let path = fetch_to_register(&pkg)?;
    Ok((pkg, path))
}

fn resolve_version(name: &str, spec: &str) -> Result<ResolvedPackage> {
    let exact = spec.trim_start_matches('=');
    if is_exact_version(exact) {
        return fetch_specific(name, exact);
    }

    let manifest = fetch_full_manifest(name)?;

    let version = match spec {
        "" | "*" | "latest" => manifest
            .dist_tags
            .get("latest")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no 'latest' tag for {name}"))?,
        _ if spec.contains("||") => {
            // Multiple ranges — pick latest for simplicity
            manifest
                .dist_tags
                .get("latest")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("no 'latest' tag for {name}"))?
        }
        _ => pick_semver_range(&manifest, spec)?,
    };

    let meta = manifest
        .versions
        .get(&version)
        .ok_or_else(|| anyhow::anyhow!("version {version} not in registry for {name}"))?;

    Ok(make_resolved(&manifest.name, &version, meta))
}

fn is_exact_version(s: &str) -> bool {
    s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
}

fn fetch_specific(name: &str, version: &str) -> Result<ResolvedPackage> {
    let url = format!("https://registry.npmjs.org/{}/{version}", npm_path(name));
    let meta: NpmVersionMeta = get_json(&url)?;
    Ok(make_resolved(name, &meta.version.clone(), &meta))
}

fn fetch_full_manifest(name: &str) -> Result<NpmFullManifest> {
    let url = format!("https://registry.npmjs.org/{}", npm_path(name));
    get_json(&url)
}

fn make_resolved(name: &str, version: &str, meta: &NpmVersionMeta) -> ResolvedPackage {
    ResolvedPackage {
        name: name.to_string(),
        version: version.to_string(),
        tarball_url: meta.dist.tarball.clone(),
        integrity: meta.dist.integrity.clone(),
        dependencies: meta.dependencies.clone(),
        bin: std::mem::take(&mut meta.bin.clone().into_map(name)),
        main: meta.main.clone(),
    }
}

// NpmVersionMeta.bin is not Clone — work around by re-deriving:
impl NpmBin {
    fn clone(&self) -> Self {
        match self {
            NpmBin::None => NpmBin::None,
            NpmBin::Single(s) => NpmBin::Single(s.clone()),
            NpmBin::Map(m) => NpmBin::Map(m.clone()),
        }
    }
}

fn pick_semver_range(manifest: &NpmFullManifest, spec: &str) -> Result<String> {
    let (kind, base) = if let Some(r) = spec.strip_prefix('^') {
        ("caret", r.trim())
    } else if let Some(r) = spec.strip_prefix('~') {
        ("tilde", r.trim())
    } else if let Some(r) = spec.strip_prefix(">=") {
        ("gte", r.trim())
    } else if let Some(r) = spec.strip_prefix('>') {
        ("gt", r.trim())
    } else if let Some(r) = spec.strip_prefix("<=") {
        ("lte", r.trim())
    } else {
        // Unknown prefix — try as exact
        return Ok(spec.to_string());
    };

    let base = parse_semver(base).unwrap_or((0, 0, 0));

    let mut candidates: Vec<(u64, u64, u64, String)> = manifest
        .versions
        .keys()
        .filter_map(|v| {
            let p = parse_semver(v).ok()?;
            let ok = match kind {
                "caret" => p.0 == base.0 && p >= base,
                "tilde" => p.0 == base.0 && p.1 == base.1 && p.2 >= base.2,
                "gte" => p >= base,
                "gt" => p > base,
                "lte" => p <= base,
                _ => false,
            };
            if ok { Some((p.0, p.1, p.2, v.clone())) } else { None }
        })
        .collect();

    candidates.sort();
    candidates
        .last()
        .map(|(_, _, _, v)| v.clone())
        .ok_or_else(|| anyhow::anyhow!("no version matching '{}' for {}", spec, manifest.name))
}

fn parse_semver(s: &str) -> Result<(u64, u64, u64)> {
    let base = s.split('-').next().unwrap_or(s);
    let mut parts = base.split('.');
    let major: u64 = parts.next().unwrap_or("0").parse().unwrap_or(0);
    let minor: u64 = parts.next().unwrap_or("0").parse().unwrap_or(0);
    let patch: u64 = parts.next().unwrap_or("0").parse().unwrap_or(0);
    Ok((major, minor, patch))
}

/// Download package tarball and extract to `~/.rts/register/npm/<name>/<version>/`.
pub fn fetch_to_register(pkg: &ResolvedPackage) -> Result<PathBuf> {
    let dest = register_dir()?
        .join("npm")
        .join(safe_segment(&pkg.name))
        .join(&pkg.version);

    if dest.exists() {
        return Ok(dest);
    }

    std::fs::create_dir_all(&dest)
        .with_context(|| format!("create register dir {}", dest.display()))?;

    let bytes = download_bytes(&pkg.tarball_url)?;

    let gz = flate2::read::GzDecoder::new(std::io::Cursor::new(&bytes));
    let mut archive = tar::Archive::new(gz);

    for entry in archive.entries().context("read tarball entries")? {
        let mut entry = entry.context("read tarball entry")?;
        let raw_path = entry.path().context("entry path")?;
        let rel = match raw_path.strip_prefix("package") {
            Ok(r) => r.to_path_buf(),
            Err(_) => raw_path.to_path_buf(),
        };
        let out = dest.join(&rel);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        entry.unpack(&out).ok();
    }

    Ok(dest)
}

fn npm_path(name: &str) -> String {
    // @scope/pkg → @scope%2Fpkg for registry URL
    if let Some(rest) = name.strip_prefix('@') {
        if let Some(slash) = rest.find('/') {
            let scope = &rest[..slash];
            let pkg = &rest[slash + 1..];
            return format!("@{scope}%2F{pkg}");
        }
    }
    name.to_string()
}

pub fn safe_segment(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn get_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T> {
    let resp = ureq::get(url)
        .set("Accept", "application/json")
        .call()
        .with_context(|| format!("GET {url}"))?;
    let body = resp.into_string().context("read response body")?;
    serde_json::from_str(&body).with_context(|| format!("parse JSON from {url}"))
}

fn download_bytes(url: &str) -> Result<Vec<u8>> {
    let resp = ureq::get(url)
        .call()
        .with_context(|| format!("download {url}"))?;
    let mut buf = Vec::new();
    resp.into_reader()
        .read_to_end(&mut buf)
        .with_context(|| format!("read response from {url}"))?;
    Ok(buf)
}
