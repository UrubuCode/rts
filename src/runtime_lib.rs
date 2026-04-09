use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use sha2::{Digest, Sha256};

pub(crate) const RUNTIME_LIB_DOWNLOAD_URL_ENV_VAR: &str = "RTS_RUNTIME_LIB_DOWNLOAD_URL";
pub(crate) const RUNTIME_LIB_SHA256_ENV_VAR: &str = "RTS_RUNTIME_LIB_SHA256";
pub(crate) const RUNTIME_LIB_TOOL_NAME: &str = "rts-runtime";

pub(crate) fn resolve_runtime_support_library(deps_dir: &Path) -> Result<PathBuf> {
    if let Some(path) = find_runtime_support_library(deps_dir) {
        return Ok(path);
    }

    match maybe_download_runtime_support_library() {
        Ok(Some(path)) => return Ok(path),
        Ok(None) => {}
        Err(error) => {
            eprintln!(
                "RTS runtime: failed to download runtime support library from web ({}). Falling back to local build.",
                error
            );
        }
    }

    build_runtime_support_library()?;

    if let Some(path) = find_runtime_support_library(deps_dir) {
        return Ok(path);
    }

    Err(anyhow!(
        "RTS runtime support library was not found (tried: {}). Run `cargo build --release --lib` or configure {}.",
        runtime_staticlib_names().join(", "),
        RUNTIME_LIB_DOWNLOAD_URL_ENV_VAR
    ))
}

fn find_runtime_support_library(deps_dir: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::<PathBuf>::new();
    let library_names = runtime_staticlib_names();

    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            for name in &library_names {
                candidates.push(parent.join(name));
            }
        }
    }

    for name in &library_names {
        candidates.push(PathBuf::from("target").join("release").join(name));
        candidates.push(PathBuf::from("target").join("debug").join(name));
        candidates.push(deps_dir.join(name));
    }

    if let Ok(base) = crate::linker::toolchain::toolchains_base_dir() {
        let target = crate::linker::toolchain::TargetTriple::resolve(None);
        for name in &library_names {
            candidates.push(
                base.join(RUNTIME_LIB_TOOL_NAME)
                    .join(&target.triple)
                    .join(name),
            );
        }
    }

    candidates.dedup();
    candidates.into_iter().find(|candidate| candidate.is_file())
}

fn maybe_download_runtime_support_library() -> Result<Option<PathBuf>> {
    let Some(template) = std::env::var(RUNTIME_LIB_DOWNLOAD_URL_ENV_VAR)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    let target = crate::linker::toolchain::TargetTriple::resolve(None);
    let lib_name =
        runtime_staticlib_names()
            .first()
            .copied()
            .unwrap_or(if cfg!(target_os = "windows") {
                "rts.lib"
            } else {
                "librts.a"
            });

    let cache_dir = crate::linker::toolchain::toolchains_base_dir()?
        .join(RUNTIME_LIB_TOOL_NAME)
        .join(&target.triple);
    std::fs::create_dir_all(&cache_dir)
        .with_context(|| format!("failed to create {}", cache_dir.display()))?;
    let destination = cache_dir.join(lib_name);

    if destination.is_file() {
        return Ok(Some(destination));
    }

    let url = template
        .replace("{target}", &target.triple)
        .replace("{lib}", lib_name);
    eprintln!(
        "RTS runtime: getting '{}' for '{}' from web...",
        lib_name, target.triple
    );
    let bytes = download_bytes_from_web(&url)?;

    if let Some(expected) = std::env::var(RUNTIME_LIB_SHA256_ENV_VAR)
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    {
        verify_sha256_bytes(&bytes, &expected, &url)?;
    }

    std::fs::write(&destination, &bytes).with_context(|| {
        format!(
            "failed to write downloaded runtime lib {}",
            destination.display()
        )
    })?;
    Ok(Some(destination))
}

fn download_bytes_from_web(url: &str) -> Result<Vec<u8>> {
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
        Err(ureq::Error::Transport(error)) => bail!("failed to download {} ({})", url, error),
    };

    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read downloaded body from {}", url))?;
    Ok(bytes)
}

fn verify_sha256_bytes(bytes: &[u8], expected: &str, label: &str) -> Result<()> {
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

fn build_runtime_support_library() -> Result<()> {
    let release = std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.parent()
                .and_then(|parent| parent.file_name())
                .and_then(|value| value.to_str())
                .map(|value| value.eq_ignore_ascii_case("release"))
        })
        .unwrap_or(true);

    let mut command = Command::new("cargo");
    command.arg("build").arg("--lib");
    if release {
        command.arg("--release");
    }

    let output = command
        .output()
        .context("failed to invoke cargo to build RTS runtime support library")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        bail!(
            "failed to build RTS runtime support library (status={:?}, stdout='{}', stderr='{}')",
            output.status.code(),
            stdout,
            stderr
        );
    }

    Ok(())
}

pub(crate) fn runtime_staticlib_names() -> Vec<&'static str> {
    if cfg!(target_os = "windows") {
        vec!["rts.lib", "librts.lib"]
    } else {
        vec!["librts.a", "rts.a"]
    }
}
