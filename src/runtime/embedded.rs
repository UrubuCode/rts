//! Runtime static-library handoff between the `bin` target and the `lib`.
//!
//! The binary embeds `librts.a`/`rts.lib` via `include_bytes!` and installs
//! the resulting slice here before dispatching to the CLI. The library then
//! reads the bytes through [`runtime_staticlib`] and caches an extracted
//! copy in a content-addressed location.
//!
//! Why the indirection: if the `include_bytes!` call lived inside the lib,
//! it would be compiled into the static library itself, producing a
//! self-including archive that doubles in size with every rebuild. Keeping
//! the embed in the bin-only world avoids that recursion entirely.

use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

static EMBEDDED: OnceLock<EmbeddedRuntime> = OnceLock::new();

#[derive(Debug, Clone, Copy)]
pub struct EmbeddedRuntime {
    pub bytes: &'static [u8],
    pub extension: &'static str,
}

/// Installs the embedded runtime slice. Called exactly once from the
/// binary entry point; subsequent calls are ignored.
pub fn install(runtime: EmbeddedRuntime) {
    let _ = EMBEDDED.set(runtime);
}

/// Returns the embedded runtime, or an error when the binary forgot to
/// install it (e.g. when the lib is consumed directly by tests).
pub fn runtime_staticlib() -> Result<EmbeddedRuntime> {
    EMBEDDED
        .get()
        .copied()
        .context("embedded RTS runtime has not been installed by the binary")
}

/// Writes the embedded library to a cache directory and returns the path.
///
/// The cache key is the sha256 of the embedded bytes so reinstalled RTS
/// versions never collide. Empty embeds return an error with a rebuild hint.
pub fn extract_runtime_staticlib() -> Result<PathBuf> {
    let runtime = runtime_staticlib()?;
    if runtime.bytes.is_empty() {
        bail!(
            "embedded RTS runtime library is empty — this usually means the \
             RTS binary was produced on a tree where the static library was \
             not yet built. Rebuild with `cargo build --release` a second \
             time to capture the staticlib, then retry."
        );
    }

    let mut hasher = Sha256::new();
    hasher.update(runtime.bytes);
    let digest = hex_digest(&hasher.finalize());

    let cache_root = cache_dir();
    std::fs::create_dir_all(&cache_root)
        .with_context(|| format!("failed to create {}", cache_root.display()))?;

    let file_name = format!("rts_runtime_{digest}.{}", runtime.extension);
    let target = cache_root.join(file_name);

    if target.exists() && file_matches(&target, runtime.bytes) {
        return Ok(target);
    }

    std::fs::write(&target, runtime.bytes)
        .with_context(|| format!("failed to write {}", target.display()))?;
    Ok(target)
}

fn cache_dir() -> PathBuf {
    if let Some(explicit) = std::env::var_os("RTS_CACHE_DIR") {
        return PathBuf::from(explicit);
    }
    std::env::temp_dir().join("rts-runtime-cache")
}

fn file_matches(path: &std::path::Path, expected: &[u8]) -> bool {
    match std::fs::read(path) {
        Ok(existing) => existing == expected,
        Err(_) => false,
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(nibble(byte >> 4));
        out.push(nibble(byte & 0x0f));
    }
    out
}

fn nibble(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'a' + (value - 10)) as char,
        _ => '0',
    }
}
