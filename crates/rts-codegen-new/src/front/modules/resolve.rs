//! Specifier resolution for the new engine's module system (M1a).
//!
//! Turns an import specifier (the string in `from "..."`) into a [`Target`]:
//!
//! - **relative** (`./x`, `../x`) → a canonical absolute [`Target::File`] on
//!   disk, trying the candidate list `x.ts, x.rts, x.js, x/index.ts,
//!   x/index.rts, x/index.js` (mirrors the old engine's
//!   `import_resolver::resolve_source_candidate`);
//! - **builtin** (`rts`, `rts:<ns>`, `node:<ns>`) → a [`Target::Builtin`]
//!   carrying the original specifier, WITHOUT touching disk (the actual
//!   member-symbol resolution is the Registry's job at lowering time, M1b);
//! - **bare non-builtin** (npm / workspace package names) → an explicit
//!   [`Target::Unsupported`] — out of M1 scope, an honest bail, never a guess.
//!
//! This module is PURE w.r.t. process state: it only reads the filesystem for
//! the relative branch. No network, no manifest/workspace walking (those were
//! old-engine features; the new engine re-introduces them in M2/M3 if needed).

use std::path::{Path, PathBuf};

use super::error::{ModuleError, ModuleResult};

/// The outcome of resolving one import specifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A user source file on disk, resolved to its canonical absolute path.
    File(PathBuf),
    /// A builtin module (`rts`, `rts:<ns>`, `node:<ns>`). Carries the original
    /// specifier; the member binding is resolved later via the Registry.
    Builtin { specifier: String },
    /// A bare non-builtin specifier (npm / workspace package). Out of M1 scope.
    Unsupported { specifier: String },
}

/// The candidate file extensions / index files tried for a relative import with
/// no explicit extension, in precedence order (same list as the old engine).
const CANDIDATE_EXTS: [&str; 3] = ["ts", "rts", "js"];
const INDEX_FILES: [&str; 3] = ["index.ts", "index.rts", "index.js"];

/// `true` if `specifier` names a builtin module reachable WITHOUT disk access:
/// `rts`, any `rts:<ns>`, or any `node:<ns>`.
pub fn is_builtin(specifier: &str) -> bool {
    specifier == "rts" || specifier.starts_with("rts:") || specifier.starts_with("node:")
}

/// `true` if `specifier` is a filesystem-relative import (`./` or `../`).
pub fn is_relative(specifier: &str) -> bool {
    specifier.starts_with("./") || specifier.starts_with("../")
}

/// Resolve `specifier` referenced from the module at `from_file` into a
/// [`Target`].
///
/// `from_file` is the canonical absolute path of the importing module; relative
/// specifiers resolve against its parent directory.
pub fn resolve(specifier: &str, from_file: &Path) -> ModuleResult<Target> {
    if is_builtin(specifier) {
        return Ok(Target::Builtin {
            specifier: specifier.to_string(),
        });
    }

    if is_relative(specifier) {
        let base = from_file.parent().ok_or_else(|| {
            ModuleError::Resolve(format!(
                "importing module has no parent directory: {}",
                from_file.display()
            ))
        })?;
        let path = resolve_relative(base, specifier)?;
        return Ok(Target::File(path));
    }

    // Bare non-builtin specifier (npm/workspace) — honest bail, out of M1 scope.
    Ok(Target::Unsupported {
        specifier: specifier.to_string(),
    })
}

/// Resolve the entry path of the whole program (the file the user passed). Tries
/// the bare path, then `.ts/.rts/.js`, returning the canonical absolute path.
pub fn resolve_entry(entry: &Path) -> ModuleResult<PathBuf> {
    if entry.is_file() {
        return canonicalize(entry);
    }
    if entry.extension().is_none() {
        for ext in CANDIDATE_EXTS {
            let cand = entry.with_extension(ext);
            if cand.is_file() {
                return canonicalize(&cand);
            }
        }
    }
    Err(ModuleError::Resolve(format!(
        "entry module not found: {}",
        entry.display()
    )))
}

/// Resolve a relative `specifier` against `base_dir`, trying the candidate list.
fn resolve_relative(base_dir: &Path, specifier: &str) -> ModuleResult<PathBuf> {
    let candidate = base_dir.join(specifier);
    let mut attempts: Vec<PathBuf> = Vec::new();

    if candidate.extension().is_some() {
        // Explicit extension — try as-is only.
        attempts.push(candidate.clone());
    } else {
        for ext in CANDIDATE_EXTS {
            attempts.push(candidate.with_extension(ext));
        }
        for idx in INDEX_FILES {
            attempts.push(candidate.join(idx));
        }
    }

    for path in &attempts {
        if path.is_file() {
            return canonicalize(path);
        }
    }

    Err(ModuleError::Resolve(format!(
        "module not found: '{specifier}' (resolved from {})",
        base_dir.display()
    )))
}

/// Canonicalize a path (resolving `.`/`..`/symlinks) so two specifiers naming
/// the same file dedup to ONE key. Wraps the IO error as a `ModuleError`.
pub fn canonicalize(path: &Path) -> ModuleResult<PathBuf> {
    path.canonicalize()
        .map_err(|e| ModuleError::Io(format!("failed to canonicalize {}: {e}", path.display())))
}
