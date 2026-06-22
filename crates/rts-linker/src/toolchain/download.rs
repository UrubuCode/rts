//! Optional download of a target linker (env-templated URL or Rust dist rustc).

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use super::paths::{
    cache_destination_for_tool, expected_binary_name, mirror_to_legacy_layout,
    sanitize_tool_dir_name, set_executable_if_supported,
};
use super::target::ToolchainLayout;

pub(crate) const LINKER_DOWNLOAD_URL_ENV_VAR: &str = "RTS_LINKER_DOWNLOAD_URL";
const LINKER_SHA256_ENV_VAR: &str = "RTS_LINKER_SHA256";
const RUST_DIST_MANIFEST_URL: &str = "https://static.rust-lang.org/dist/channel-rust-stable.toml";
pub(crate) const RUST_LLD_TOOL_NAME: &str = "rust-lld";

pub(crate) fn maybe_download_linker(
    layout: &ToolchainLayout,
    binary_name: &str,
    toolchains_base: &Path,
) -> Result<Option<PathBuf>> {
    let Some(template) = std::env::var(LINKER_DOWNLOAD_URL_ENV_VAR)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    let binary_file = expected_binary_name(binary_name);
    let url = template
        .replace("{target}", &layout.target.triple)
        .replace("{binary}", &binary_file);

    let destination = cache_destination_for_tool(
        toolchains_base,
        sanitize_tool_dir_name(binary_name).as_str(),
        &layout.target.triple,
        &binary_file,
    )?;
    mirror_to_legacy_layout(&layout.bin_dir, &binary_file, &destination)?;
    if destination.is_file() {
        eprintln!(
            "RTS toolchain: using cached target '{}' from {}",
            layout.target.triple,
            destination.display()
        );
        return Ok(Some(destination));
    }

    eprintln!(
        "RTS toolchain: getting target '{}' linker from web...",
        layout.target.triple
    );
    let bytes = download_url_bytes(&url)?;

    if let Some(expected) = std::env::var(LINKER_SHA256_ENV_VAR)
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    {
        verify_sha256(&bytes, &expected, &url)?;
    }

    std::fs::write(&destination, &bytes).with_context(|| {
        format!(
            "failed to write downloaded linker {}",
            destination.display()
        )
    })?;
    mirror_to_legacy_layout(&layout.bin_dir, &binary_file, &destination)?;
    set_executable_if_supported(&destination)?;

    eprintln!(
        "RTS toolchain: target '{}' linker downloaded and cached.",
        layout.target.triple
    );

    Ok(Some(destination))
}

pub(crate) fn maybe_download_rust_dist_linker(
    layout: &ToolchainLayout,
    toolchains_base: &Path,
) -> Result<Option<PathBuf>> {
    let binary_name = expected_binary_name("rust-lld");
    let destination = cache_destination_for_tool(
        toolchains_base,
        RUST_LLD_TOOL_NAME,
        &layout.target.triple,
        &binary_name,
    )?;
    mirror_to_legacy_layout(&layout.bin_dir, &binary_name, &destination)?;
    if destination.is_file() {
        eprintln!(
            "RTS toolchain: using cached target '{}' from {}",
            layout.target.triple,
            destination.display()
        );
        return Ok(Some(destination));
    }

    let Some(artifact) = rust_dist_rustc_artifact_for_target(&layout.target.triple)? else {
        return Ok(None);
    };

    eprintln!(
        "RTS toolchain: getting target '{}' from Rust dist...",
        layout.target.triple
    );
    let archive_bytes = download_url_bytes(&artifact.url)?;
    verify_sha256(&archive_bytes, &artifact.hash, &artifact.url)?;

    if !extract_rust_lld_from_rustc_archive(&archive_bytes, &destination)? {
        bail!(
            "downloaded Rust dist archive did not contain rust-lld for target {} ({})",
            layout.target.triple,
            artifact.url
        );
    }
    mirror_to_legacy_layout(&layout.bin_dir, &binary_name, &destination)?;

    eprintln!(
        "RTS toolchain: target '{}' downloaded and cached.",
        layout.target.triple
    );

    Ok(Some(destination))
}

#[derive(Debug, Clone)]
struct RustDistArtifact {
    url: String,
    hash: String,
}

fn rust_dist_rustc_artifact_for_target(target: &str) -> Result<Option<RustDistArtifact>> {
    let manifest_bytes = download_url_bytes(RUST_DIST_MANIFEST_URL)?;
    let manifest = String::from_utf8(manifest_bytes)
        .with_context(|| format!("failed to decode {}", RUST_DIST_MANIFEST_URL))?;

    let header = format!("[pkg.rustc.target.{target}]");
    let mut in_section = false;
    let mut available = None::<bool>;
    let mut url = None::<String>;
    let mut hash = None::<String>;

    for raw_line in manifest.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') {
            if in_section {
                break;
            }
            in_section = line == header;
            continue;
        }

        if !in_section {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();

        match key {
            "available" => {
                available = Some(value.eq_ignore_ascii_case("true"));
            }
            "url" => {
                if let Some(parsed) = parse_toml_string(value) {
                    url = Some(parsed);
                }
            }
            "hash" => {
                if let Some(parsed) = parse_toml_string(value) {
                    hash = Some(parsed.to_ascii_lowercase());
                }
            }
            _ => {}
        }
    }

    if !in_section {
        return Ok(None);
    }

    if !available.unwrap_or(false) {
        return Ok(None);
    }

    match (url, hash) {
        (Some(url), Some(hash)) => Ok(Some(RustDistArtifact { url, hash })),
        _ => Ok(None),
    }
}

fn parse_toml_string(raw: &str) -> Option<String> {
    let raw = raw.trim();
    raw.strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .map(|value| value.to_string())
}

fn extract_rust_lld_from_rustc_archive(archive_bytes: &[u8], destination: &Path) -> Result<bool> {
    let decoder = flate2::read::GzDecoder::new(Cursor::new(archive_bytes));
    let mut archive = tar::Archive::new(decoder);

    for entry in archive
        .entries()
        .context("failed to read Rust dist archive")?
    {
        let mut entry = entry.context("failed to read Rust dist archive entry")?;
        let path = entry
            .path()
            .context("failed to read Rust dist archive entry path")?;
        let normalized = path.to_string_lossy().replace('\\', "/");

        if normalized.ends_with("/bin/rust-lld") || normalized.ends_with("/bin/rust-lld.exe") {
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }

            let mut file = std::fs::File::create(destination)
                .with_context(|| format!("failed to create {}", destination.display()))?;
            std::io::copy(&mut entry, &mut file)
                .with_context(|| format!("failed to extract {}", destination.display()))?;
            set_executable_if_supported(destination)?;
            return Ok(true);
        }
    }

    Ok(false)
}

fn download_url_bytes(url: &str) -> Result<Vec<u8>> {
    let response = match ureq::get(url).timeout(Duration::from_secs(90)).call() {
        Ok(value) => value,
        Err(ureq::Error::Status(code, response)) => {
            bail!(
                "failed to download {} (HTTP {} {})",
                url,
                code,
                response.status_text()
            )
        }
        Err(ureq::Error::Transport(error)) => {
            bail!("failed to download {} ({})", url, error)
        }
    };

    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read downloaded body from {}", url))?;
    Ok(bytes)
}

fn verify_sha256(bytes: &[u8], expected: &str, label: &str) -> Result<()> {
    let digest = Sha256::digest(bytes);
    let actual = format!("{digest:x}");
    if actual != expected.to_ascii_lowercase() {
        bail!(
            "SHA-256 mismatch for {} (expected {}, got {})",
            label,
            expected,
            actual
        );
    }
    Ok(())
}
