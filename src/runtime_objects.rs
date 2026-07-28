use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

/// zstd-compressed combined Rust staticlib for all runtime namespaces
/// (gc + io + fs + …) for the **host** target. Compiled + compressed at build
/// time; decompressed to `~/.rts/artifacts/<host-triple>.a` on demand. Keeps the
/// shipped `rts` binary small (~99MB raw -> ~18MB embedded).
static RUNTIME_ARCHIVE_ZST: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/runtime_support.a.zst"));

/// sha256 (hex) of the *decompressed* archive, computed by `build.rs`.
/// The sentinel `PLACEHOLDER` means the rts-runtime staticlib was not available
/// at compile time (AOT will bail with a clear message; JIT is unaffected).
static RUNTIME_ARCHIVE_SHA: &str =
    include_str!(concat!(env!("OUT_DIR"), "/runtime_support.sha256"));

/// Target triple the embedded archive was compiled for (the build host). Emitted
/// by `build.rs` from Cargo's `TARGET`. Cross targets are NOT embedded — they are
/// resolved from prebuilt per-target archives (see `ensure_cross_artifact`).
const HOST_TARGET: &str = env!("RTS_HOST_TARGET");

/// Directory of prebuilt per-target archives (`<dir>/<triple>.a`). Set by users
/// who cross-compile and ship their own runtime archives.
const RUNTIME_OBJECTS_DIR_ENV: &str = "RTS_RUNTIME_OBJECTS_DIR";

/// Optional URL template for downloading a prebuilt archive on demand. The
/// `{target}` placeholder is replaced with the triple. CI publishes per-target
/// archives that this can point at.
const RUNTIME_ARCHIVE_URL_ENV: &str = "RTS_RUNTIME_ARCHIVE_URL";

/// Returns the runtime archive for the **host** target (the common case).
pub(crate) fn ensure_artifacts() -> Result<PathBuf> {
    ensure_artifacts_for_target(HOST_TARGET)
}

/// Returns `~/.rts/artifacts/<target>.a`, materializing it on demand.
///
/// - **host target**: decompress the archive embedded in this `rts` binary.
/// - **cross target**: locate a prebuilt `<target>.a` (env dir, then a
///   `runtime-objects/` folder next to the executable, then an optional download
///   URL), because a single `rts` binary only embeds its host archive.
///
/// This is a global user-level cache shared by all projects; re-materialization
/// of the host archive only happens when `rts` itself is rebuilt with a new
/// runtime (detected via the embedded sha256).
pub(crate) fn ensure_artifacts_for_target(target: &str) -> Result<PathBuf> {
    let target = target.trim();
    if target.is_empty() {
        bail!("empty target triple for runtime artifacts");
    }

    let path = artifact_path(target)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create artifacts dir {}", parent.display()))?;
    }

    if target == HOST_TARGET {
        ensure_host_artifact(&path)
    } else {
        ensure_cross_artifact(target, &path)
    }
}

/// Host path: decompress the embedded archive when the on-disk file is missing or
/// differs from the embedded one (sha256 compare).
fn ensure_host_artifact(path: &Path) -> Result<PathBuf> {
    let expected = RUNTIME_ARCHIVE_SHA.trim();
    if expected == "PLACEHOLDER" {
        bail!(
            "runtime archive is a placeholder — it was not built when `rts` was \
             compiled. AOT (`rts compile`) needs the full runtime staticlib. \
             Build it with `cargo build -p rts-runtime` (matching the profile), \
             then rebuild `rts`. JIT (`rts run`) does not require this."
        );
    }

    let up_to_date = if path.is_file() {
        let on_disk = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
        sha256_hex(&on_disk) == expected
    } else {
        false
    };

    if !up_to_date {
        let bytes = decompress(RUNTIME_ARCHIVE_ZST)?;
        std::fs::write(path, &bytes).with_context(|| format!("write {}", path.display()))?;
    }

    Ok(path.to_path_buf())
}

/// Cross path: a single `rts` binary only embeds its host archive, so a prebuilt
/// `<target>.a` must be provided externally.
fn ensure_cross_artifact(target: &str, path: &Path) -> Result<PathBuf> {
    if path.is_file() {
        return Ok(path.to_path_buf());
    }

    if let Some(src) = locate_prebuilt(target)? {
        std::fs::copy(&src, path)
            .with_context(|| format!("copy {} -> {}", src.display(), path.display()))?;
        return Ok(path.to_path_buf());
    }

    if let Some(template) = std::env::var(RUNTIME_ARCHIVE_URL_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        let url = template.replace("{target}", target);
        let bytes = download_url_bytes(&url)?;
        std::fs::write(path, &bytes).with_context(|| format!("write {}", path.display()))?;
        return Ok(path.to_path_buf());
    }

    bail!(
        "no prebuilt runtime archive for cross target '{target}'. This `rts` only \
         embeds the host archive ('{HOST_TARGET}'). Provide a `<target>.a` via \
         ${RUNTIME_OBJECTS_DIR_ENV}, a `runtime-objects/{target}.a` next to the \
         `rts` executable, or set ${RUNTIME_ARCHIVE_URL_ENV} (with a {{target}} \
         placeholder). Per-target archives are published by CI \
         (cargo build -p rts-runtime --target {target})."
    )
}

/// Looks for a prebuilt `<target>.a` in the env-configured directory, then in a
/// `runtime-objects/` folder next to the running executable.
fn locate_prebuilt(target: &str) -> Result<Option<PathBuf>> {
    let file = format!("{target}.a");

    if let Some(dir) = std::env::var(RUNTIME_OBJECTS_DIR_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        let candidate = PathBuf::from(dir).join(&file);
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("runtime-objects").join(&file);
            if candidate.is_file() {
                return Ok(Some(candidate));
            }
        }
    }

    Ok(None)
}

fn download_url_bytes(url: &str) -> Result<Vec<u8>> {
    let response = ureq::get(url)
        .call()
        .with_context(|| format!("download runtime archive from {url}"))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .with_context(|| format!("read runtime archive body from {url}"))?;
    Ok(bytes)
}

fn decompress(zst: &[u8]) -> Result<Vec<u8>> {
    zstd::decode_all(zst).context("decompress embedded runtime archive")
}

fn artifact_path(target: &str) -> Result<PathBuf> {
    Ok(crate::registers::rts_home()?
        .join("artifacts")
        .join(format!("{target}.a")))
}

fn sha256_hex(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
