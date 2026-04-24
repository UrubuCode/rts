//! `rts run <input.ts>` — compile + link to a cached binary and execute it.
//!
//! The output binary lives inside the project tree (typically
//! `<project>/node_modules/.rts/bin/`), not in `%TEMP%`. This matters on
//! Windows: fresh executables under `%TEMP%` get scanned by the system
//! antivirus before each launch, adding ~300 ms of fixed overhead per
//! `rts run` invocation. A project-local directory is normally on the
//! developer's AV exclusion list, so the first launch is nearly as fast
//! as a native executable.
//!
//! Repeated invocations are further accelerated by a fingerprint cache:
//! the compiled binary is only rebuilt when the source bytes, compiler
//! options, or `rts` executable itself change. A matching cache entry
//! turns `rts run` into a near-zero-cost wrapper around executing the
//! cached binary directly.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use sha2::{Digest, Sha256};

use crate::compile_options::CompileOptions;
use crate::pipeline;

/// Marker file sitting next to the cached binary. Contains the
/// fingerprint of the inputs that produced the binary; when it matches
/// the current inputs, the compile step is skipped.
const FINGERPRINT_SUFFIX: &str = ".fp";

pub fn command(input: Option<String>, options: CompileOptions) -> Result<()> {
    let input = input.ok_or_else(|| anyhow!("usage: rts run <input.ts>"))?;
    let input_path = PathBuf::from(&input);
    if !input_path.exists() {
        return Err(anyhow!("input file not found: {}", input_path.display()));
    }

    let out_path = run_binary_path(&input_path)?;
    let fingerprint = compute_fingerprint(&input_path, options)?;

    if cache_hit(&out_path, &fingerprint) {
        let status = Command::new(&out_path)
            .status()
            .with_context(|| format!("failed to execute {}", out_path.display()))?;
        std::process::exit(status.code().unwrap_or(0));
    }

    let outcome = pipeline::build_executable(&input_path, &out_path, options)
        .with_context(|| format!("compile of {} failed", input_path.display()))?;

    if options.debug {
        for warning in &outcome.compile.warnings {
            eprintln!("warning: {warning}");
        }
    }

    // Record the fingerprint so the next invocation can skip compilation.
    // Failure to write is non-fatal — it just means we recompile next time.
    let _ = write_fingerprint(&outcome.binary.path, &fingerprint);

    let status = Command::new(&outcome.binary.path)
        .status()
        .with_context(|| format!("failed to execute {}", outcome.binary.path.display()))?;

    std::process::exit(status.code().unwrap_or(0));
}

/// Returns `true` when the cached binary at `binary_path` matches
/// `expected`. Any read error or mismatch is treated as a miss.
fn cache_hit(binary_path: &Path, expected: &str) -> bool {
    if !binary_path.is_file() {
        return false;
    }
    let fp_path = fingerprint_path(binary_path);
    match std::fs::read_to_string(&fp_path) {
        Ok(content) => content.trim() == expected,
        Err(_) => false,
    }
}

fn write_fingerprint(binary_path: &Path, fingerprint: &str) -> Result<()> {
    let fp_path = fingerprint_path(binary_path);
    std::fs::write(&fp_path, fingerprint)
        .with_context(|| format!("failed to write fingerprint {}", fp_path.display()))
}

fn fingerprint_path(binary_path: &Path) -> PathBuf {
    let mut s = binary_path.as_os_str().to_os_string();
    s.push(FINGERPRINT_SUFFIX);
    PathBuf::from(s)
}

/// Hash the inputs that influence compilation output: source bytes,
/// serialized `CompileOptions`, and the `rts` binary's own mtime+size
/// (so upgrading the compiler invalidates the cache). Source-level
/// imports are not yet tracked; they will be added when the module
/// graph pipeline lands.
fn compute_fingerprint(input: &Path, options: CompileOptions) -> Result<String> {
    let source = std::fs::read(input)
        .with_context(|| format!("failed to read {} for fingerprint", input.display()))?;

    let mut hasher = Sha256::new();
    hasher.update(b"rts-run-fp-v1\n");
    hasher.update((source.len() as u64).to_le_bytes());
    hasher.update(&source);

    // Options that change codegen output.
    hasher.update(b"options\n");
    hasher.update(format!("{options:?}").as_bytes());

    // `rts` executable signature — upgrading the compiler invalidates
    // all cache entries. Use (len, mtime) for speed; a hash is overkill
    // here since we just need a cheap version tag.
    if let Ok(exe) = std::env::current_exe() {
        if let Ok(meta) = std::fs::metadata(&exe) {
            hasher.update(b"rts-exe\n");
            hasher.update(meta.len().to_le_bytes());
            if let Ok(mtime) = meta.modified() {
                if let Ok(dur) = mtime.duration_since(std::time::UNIX_EPOCH) {
                    hasher.update(dur.as_nanos().to_le_bytes());
                }
            }
        }
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// Picks where the compiled `rts run` binary should live. Order of
/// preference:
///
/// 1. `<project>/node_modules/.rts/bin/` when a project root (a
///    directory containing `package.json`) is reachable from the input.
/// 2. `<rts.exe_dir>/.rts-run/` as a toolchain-local fallback.
/// 3. `%TEMP%/rts_run/` as a last resort.
///
/// The filename is `rts_run_<stem>` (no timestamp) so that repeated
/// invocations overwrite the same path, letting the OS and antivirus
/// skip re-scanning an already-known binary.
fn run_binary_path(input: &Path) -> Result<PathBuf> {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("rts_program");
    let filename = format!("rts_run_{stem}");

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(project_bin) = project_local_bin_dir(input) {
        candidates.push(project_bin);
    }
    if let Some(toolchain_bin) = toolchain_local_bin_dir() {
        candidates.push(toolchain_bin);
    }
    candidates.push(std::env::temp_dir().join("rts_run"));

    let mut last_err: Option<anyhow::Error> = None;
    for dir in candidates {
        if let Err(err) = std::fs::create_dir_all(&dir) {
            last_err = Some(anyhow::Error::from(err).context(format!(
                "failed to create run-binary directory {}",
                dir.display()
            )));
            continue;
        }
        let mut path = dir.join(&filename);
        #[cfg(target_os = "windows")]
        {
            path.set_extension("exe");
        }
        return Ok(path);
    }
    Err(last_err.unwrap_or_else(|| anyhow!("no writable location for rts run binary")))
}

/// Walks up from `input` looking for the nearest `package.json`, then
/// returns `<that dir>/node_modules/.rts/bin/` — the canonical project
/// artifact location per the RTS roadmap.
fn project_local_bin_dir(input: &Path) -> Option<PathBuf> {
    let start = input
        .canonicalize()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .or_else(|| input.parent().map(PathBuf::from))?;

    let mut cur: &Path = &start;
    loop {
        if cur.join("package.json").is_file() {
            return Some(cur.join("node_modules").join(".rts").join("bin"));
        }
        match cur.parent() {
            Some(parent) => cur = parent,
            None => return None,
        }
    }
}

/// Directory next to the `rts` executable itself, used when no project
/// root is found. Keeps binaries inside the installed toolchain, which
/// is typically whitelisted by antivirus just like the executable.
fn toolchain_local_bin_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let parent = exe.parent()?;
    Some(parent.join(".rts-run"))
}
