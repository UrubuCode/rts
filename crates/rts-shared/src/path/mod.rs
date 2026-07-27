//! `path` namespace — pure path manipulation, no filesystem calls.
//!
//! Os membros ABI sao declarados com `#[rtse::function]` (F7 de
//! `docs/specs/rts-macro-single-source.md`): simbolo, assinatura, `ts_signature`
//! e fn-ptr saem derivados da fn Rust. Args `&str` chegam como (ptr, len) e sao
//! reconstruidos pela propria macro; resultados-string sao internados a handles
//! GC via [`intern`].
//!
//! Todo retorno de string leva `#[ts("string")]`: e o que faz o motor reboxar o
//! `Handle` como TAG_STR. Derivado (`object`) o script receberia `[]` no lugar
//! do caminho.

use std::path::{Component, Path, PathBuf};

use rts_engine::abi::ty::{Bool, Handle};
use rts_engine::Engine;

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Joins a base path with a relative fragment.
#[rtse::function(module = "path", value = "join")]
#[ts("string")]
pub fn join(base: &str, part: &str) -> Handle {
    let joined = PathBuf::from(base).join(part);
    joined.to_str().map(intern).unwrap_or(0)
}

/// Parent directory; 0 when path has no parent (e.g. root or bare filename).
#[rtse::function(module = "path", value = "parent")]
#[ts("string")]
pub fn parent(path: &str) -> Handle {
    Path::new(path)
        .parent()
        .and_then(|p| p.to_str())
        .map(intern)
        .unwrap_or(0)
}

/// Final component of the path (file name with extension).
#[rtse::function(module = "path", value = "file_name")]
#[ts("string")]
pub fn file_name(path: &str) -> Handle {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(intern)
        .unwrap_or(0)
}

/// File name without extension.
#[rtse::function(module = "path", value = "stem")]
#[ts("string")]
pub fn stem(path: &str) -> Handle {
    Path::new(path)
        .file_stem()
        .and_then(|n| n.to_str())
        .map(intern)
        .unwrap_or(0)
}

/// File extension without leading dot; 0 when absent.
#[rtse::function(module = "path", value = "ext")]
#[ts("string")]
pub fn ext(path: &str) -> Handle {
    Path::new(path)
        .extension()
        .and_then(|n| n.to_str())
        .map(intern)
        .unwrap_or(0)
}

/// True when path is absolute for the current target.
#[rtse::function(module = "path", value = "is_absolute")]
pub fn is_absolute(path: &str) -> Bool {
    if Path::new(path).is_absolute() { 1 } else { 0 }
}

/// Removes `.` and collapses `..` without touching the filesystem.
#[rtse::function(module = "path", value = "normalize")]
#[ts("string")]
pub fn normalize(path: &str) -> Handle {
    let mut out = PathBuf::new();
    for comp in Path::new(path).components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
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
#[rtse::function(module = "path", value = "with_ext")]
#[ts("string")]
pub fn with_ext(path: &str, ext: &str) -> Handle {
    let result = Path::new(path).with_extension(ext);
    result.to_str().map(intern).unwrap_or(0)
}

/// Registra a namespace `path` no motor.
pub fn register(e: &mut Engine) {
    e.module("path", |m| {
        m.doc("Pure path manipulation — no filesystem calls.");
        m.registry(join_entry());
        m.registry(parent_entry());
        m.registry(file_name_entry());
        m.registry(stem_entry());
        m.registry(ext_entry());
        m.registry(is_absolute_entry());
        m.registry(normalize_entry());
        m.registry(with_ext_entry());
    });
}
