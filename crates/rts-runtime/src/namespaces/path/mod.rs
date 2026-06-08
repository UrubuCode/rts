//! `path` namespace — pure path manipulation, no filesystem calls.
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2b,
//! `docs/specs/rts-core-engine.md`). String params arrive as `&str` (the macro
//! expands `Str` → ptr+len and reconstructs them, returning 0 on bad input);
//! string results are interned to GC handles via [`intern`].
//!
//! `Handle`-returning members carry an explicit `ts = "...: string"` override:
//! the handle is a GC *string* handle, which the derived TS (`number`) cannot
//! know.

use std::path::{Component, Path, PathBuf};

use rts_abi::ty::{Bool, Handle};
use rts_macro::rts_namespace;

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

/// Interns `s` into the GC string pool, returning its handle.
fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Pure path manipulation — no filesystem calls.
#[rts_namespace(path)]
impl PathNs {
    /// Joins a base path with a relative fragment.
    #[rts_fn(pure, ts = "join(base: string, part: string): string")]
    pub fn join(base: Str, part: Str) -> Handle {
        let joined = PathBuf::from(base).join(part);
        joined.to_str().map(intern).unwrap_or(0)
    }

    /// Parent directory; 0 when path has no parent (e.g. root or bare filename).
    #[rts_fn(pure, ts = "parent(path: string): string")]
    pub fn parent(path: Str) -> Handle {
        Path::new(path)
            .parent()
            .and_then(|p| p.to_str())
            .map(intern)
            .unwrap_or(0)
    }

    /// Final component of the path (file name with extension).
    #[rts_fn(pure, ts = "file_name(path: string): string")]
    pub fn file_name(path: Str) -> Handle {
        Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .map(intern)
            .unwrap_or(0)
    }

    /// File name without extension.
    #[rts_fn(pure, ts = "stem(path: string): string")]
    pub fn stem(path: Str) -> Handle {
        Path::new(path)
            .file_stem()
            .and_then(|n| n.to_str())
            .map(intern)
            .unwrap_or(0)
    }

    /// File extension without leading dot; 0 when absent.
    #[rts_fn(pure, ts = "ext(path: string): string")]
    pub fn ext(path: Str) -> Handle {
        Path::new(path)
            .extension()
            .and_then(|n| n.to_str())
            .map(intern)
            .unwrap_or(0)
    }

    /// True when path is absolute for the current target.
    #[rts_fn(pure)]
    pub fn is_absolute(path: Str) -> Bool {
        if Path::new(path).is_absolute() { 1 } else { 0 }
    }

    /// Removes `.` and collapses `..` without touching the filesystem.
    #[rts_fn(pure, ts = "normalize(path: string): string")]
    pub fn normalize(path: Str) -> Handle {
        let mut out = PathBuf::new();
        for comp in Path::new(path).components() {
            match comp {
                Component::CurDir => {}
                Component::ParentDir => {
                    // Pop only a "normal" trailing component; otherwise keep the
                    // `..` (e.g. a relative path pointing outside the base).
                    let pop_ok = out
                        .components()
                        .next_back()
                        .map(|c| matches!(c, Component::Normal(_)))
                        .unwrap_or(false);
                    if pop_ok {
                        out.pop();
                    } else {
                        out.push("..");
                    }
                }
                other => out.push(other.as_os_str()),
            }
        }
        let rendered = out.to_string_lossy();
        intern(&rendered)
    }

    /// Returns the path with the extension replaced (or added).
    #[rts_fn(pure, ts = "with_ext(path: string, ext: string): string")]
    pub fn with_ext(path: Str, ext: Str) -> Handle {
        let result = Path::new(path).with_extension(ext);
        result.to_str().map(intern).unwrap_or(0)
    }
}
