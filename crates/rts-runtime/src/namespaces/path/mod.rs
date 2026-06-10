//! `path` namespace — pure path manipulation, no filesystem calls.
//!
//! Migrado pro modelo builder do `rts-engine` (Fase 2; ver `namespaces/hint`).
//! Args `Str` chegam como (ptr, len) e são reconstruídos via `from_abi`
//! (retorna 0 em null/UTF-8 inválido); resultados-string são internados a
//! handles GC via [`intern`]. Todos os membros são `pure`.

use std::path::{Component, Path, PathBuf};

use rts_engine::{sig, Engine, FnPtr, Member, MemberFlags, MemberKind};

use rts_engine::abi::str_abi::from_abi;

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Joins a base path with a relative fragment.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PATH_JOIN(
    base_ptr: *const u8,
    base_len: i64,
    part_ptr: *const u8,
    part_len: i64,
) -> u64 {
    let (Some(base), Some(part)) =
        (unsafe { from_abi(base_ptr, base_len) }, unsafe { from_abi(part_ptr, part_len) })
    else {
        return 0;
    };
    let joined = PathBuf::from(base).join(part);
    joined.to_str().map(intern).unwrap_or(0)
}

/// Parent directory; 0 when path has no parent (e.g. root or bare filename).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PATH_PARENT(path_ptr: *const u8, path_len: i64) -> u64 {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return 0;
    };
    Path::new(path)
        .parent()
        .and_then(|p| p.to_str())
        .map(intern)
        .unwrap_or(0)
}

/// Final component of the path (file name with extension).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PATH_FILE_NAME(path_ptr: *const u8, path_len: i64) -> u64 {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return 0;
    };
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(intern)
        .unwrap_or(0)
}

/// File name without extension.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PATH_STEM(path_ptr: *const u8, path_len: i64) -> u64 {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return 0;
    };
    Path::new(path)
        .file_stem()
        .and_then(|n| n.to_str())
        .map(intern)
        .unwrap_or(0)
}

/// File extension without leading dot; 0 when absent.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PATH_EXT(path_ptr: *const u8, path_len: i64) -> u64 {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return 0;
    };
    Path::new(path)
        .extension()
        .and_then(|n| n.to_str())
        .map(intern)
        .unwrap_or(0)
}

/// True when path is absolute for the current target.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PATH_IS_ABSOLUTE(path_ptr: *const u8, path_len: i64) -> i64 {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return 0;
    };
    if Path::new(path).is_absolute() { 1 } else { 0 }
}

/// Removes `.` and collapses `..` without touching the filesystem.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PATH_NORMALIZE(path_ptr: *const u8, path_len: i64) -> u64 {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return 0;
    };
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PATH_WITH_EXT(
    path_ptr: *const u8,
    path_len: i64,
    ext_ptr: *const u8,
    ext_len: i64,
) -> u64 {
    let (Some(path), Some(ext)) =
        (unsafe { from_abi(path_ptr, path_len) }, unsafe { from_abi(ext_ptr, ext_len) })
    else {
        return 0;
    };
    let result = Path::new(path).with_extension(ext);
    result.to_str().map(intern).unwrap_or(0)
}

fn pure_func(name: &str, symbol: &str, sig: rts_engine::Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: true,
        intrinsic: None,
    }
}

/// Registra a namespace `path` no motor (Fase 2).
pub fn register(e: &mut Engine) {
    e.ns("path")
        .doc("Pure path manipulation — no filesystem calls.")
        .member(pure_func(
            "join",
            "__RTS_FN_NS_PATH_JOIN",
            sig!(StrPtr, StrPtr => Handle),
            "join(base: string, part: string): string",
            "Joins a base path with a relative fragment.",
            __RTS_FN_NS_PATH_JOIN as *const u8,
        ))
        .member(pure_func(
            "parent",
            "__RTS_FN_NS_PATH_PARENT",
            sig!(StrPtr => Handle),
            "parent(path: string): string",
            "Parent directory; 0 when path has no parent (e.g. root or bare filename).",
            __RTS_FN_NS_PATH_PARENT as *const u8,
        ))
        .member(pure_func(
            "file_name",
            "__RTS_FN_NS_PATH_FILE_NAME",
            sig!(StrPtr => Handle),
            "file_name(path: string): string",
            "Final component of the path (file name with extension).",
            __RTS_FN_NS_PATH_FILE_NAME as *const u8,
        ))
        .member(pure_func(
            "stem",
            "__RTS_FN_NS_PATH_STEM",
            sig!(StrPtr => Handle),
            "stem(path: string): string",
            "File name without extension.",
            __RTS_FN_NS_PATH_STEM as *const u8,
        ))
        .member(pure_func(
            "ext",
            "__RTS_FN_NS_PATH_EXT",
            sig!(StrPtr => Handle),
            "ext(path: string): string",
            "File extension without leading dot; 0 when absent.",
            __RTS_FN_NS_PATH_EXT as *const u8,
        ))
        .member(pure_func(
            "is_absolute",
            "__RTS_FN_NS_PATH_IS_ABSOLUTE",
            sig!(StrPtr => Bool),
            "is_absolute(path: string): boolean",
            "True when path is absolute for the current target.",
            __RTS_FN_NS_PATH_IS_ABSOLUTE as *const u8,
        ))
        .member(pure_func(
            "normalize",
            "__RTS_FN_NS_PATH_NORMALIZE",
            sig!(StrPtr => Handle),
            "normalize(path: string): string",
            "Removes `.` and collapses `..` without touching the filesystem.",
            __RTS_FN_NS_PATH_NORMALIZE as *const u8,
        ))
        .member(pure_func(
            "with_ext",
            "__RTS_FN_NS_PATH_WITH_EXT",
            sig!(StrPtr, StrPtr => Handle),
            "with_ext(path: string, ext: string): string",
            "Returns the path with the extension replaced (or added).",
            __RTS_FN_NS_PATH_WITH_EXT as *const u8,
        ))
        .done();
}
