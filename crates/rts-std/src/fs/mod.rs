//! `fs` namespace — filesystem operations backed by `std::fs`.
//!
//! Path args arrive as strings (reconstructed `&str`; `&str: AsRef<Path>`).
//! Byte buffers travel as a `U64` pointer cast to `*mut`/`*const u8`. Status
//! functions return `-1` on error (`on_null = -1`); `exists`/`is_*` return 0/1.
//!
//! Authored with `#[rtse::function]` — the SINGLE SOURCE OF TRUTH for symbols
//! (docs/engine/architecture.md). Two exceptions stay hand-written,
//! both explained at their definition below: the node-compat `Bool` overloads
//! (`existsSync`/`isFileSync`/`isDirectorySync`) that deliberately reuse the
//! SAME extern symbol as the `number`-returning member, and `read_text`,
//! which stores raw (possibly non-UTF-8) bytes and cannot be expressed as a
//! Rust `String` return without corrupting binary files.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};

use rts_engine::abi::ty::{Handle, I64, U64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{Entry, alloc_entry};

// `string_pool` (intern de strings GC) fica no backend (rts-runtime collector);
// resolvida via `rts_engine::gc_surface` (site único, link-time).
use rts_engine::heap::string_pool::__RTS_FN_NS_GC_STRING_NEW;

/// Reads up to `bufLen` bytes from `path` into the buffer. Count, 0 on EOF, -1 on error.
#[rtse::function(module = "fs", value = "read")]
fn read(path: &str, buf_ptr: U64, buf_len: I64) -> I64 {
    if buf_ptr == 0 || buf_len <= 0 {
        return -1;
    }
    // SAFETY: caller guarantees a writable buffer for `buf_len` bytes.
    let slot = unsafe { std::slice::from_raw_parts_mut(buf_ptr as *mut u8, buf_len as usize) };
    match File::open(path).and_then(|mut f| f.read(slot)) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

/// Reads the whole file into the buffer (truncating to `bufLen`). Bytes written, -1 on error.
#[rtse::function(module = "fs", value = "read_all")]
fn read_all(path: &str, buf_ptr: U64, buf_len: I64) -> I64 {
    if buf_ptr == 0 || buf_len <= 0 {
        return -1;
    }
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return -1,
    };
    let copy = bytes.len().min(buf_len as usize);
    // SAFETY: buffer writable for `copy <= buf_len` bytes.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf_ptr as *mut u8, copy);
    }
    copy as i64
}

/// Reads the whole file as a UTF-8 string handle. 0 on error.
///
/// Hand-written (not `#[rtse::function]`): the bytes are stored verbatim into
/// `Entry::String` with NO UTF-8 validation (binary-safe passthrough); a
/// native Rust `String` return type requires valid UTF-8, which would
/// silently corrupt a non-UTF-8 file read through this path.
#[rtse::abi("__RTS_FN_NS_FS_READ_TEXT")]
pub fn __RTS_FN_NS_FS_READ_TEXT(path_ptr: u64, path_len: i64) -> Handle {
    let path_ptr = path_ptr as *const u8;

    let path = match unsafe { rts_engine::abi::str_abi::from_abi(path_ptr, path_len) } {
        Some(s) => s,
        None => return 0,
    };
    match std::fs::read(path) {
        Ok(b) => alloc_entry(Entry::String(b)),
        Err(_) => 0,
    }
}

/// Writes `data` to `path` (truncating). Bytes written, -1 on error.
#[rtse::function(module = "fs", value = "write")]
fn write(path: &str, data: &str) -> I64 {
    match std::fs::write(path, data.as_bytes()) {
        Ok(()) => data.len() as i64,
        Err(_) => -1,
    }
}

/// Writes raw buffer bytes to `path` (truncating). Bytes written, -1 on error.
#[rtse::function(module = "fs", value = "write_bytes")]
fn write_bytes(path: &str, buf_ptr: U64, len: I64) -> I64 {
    if buf_ptr == 0 || len < 0 {
        return -1;
    }
    // SAFETY: caller contract — live data for `len` bytes.
    let data = unsafe { std::slice::from_raw_parts(buf_ptr as *const u8, len as usize) };
    match std::fs::write(path, data) {
        Ok(()) => len,
        Err(_) => -1,
    }
}

/// Appends `data` to `path` (creating it if missing). Bytes written, -1 on error.
#[rtse::function(module = "fs", value = "append")]
fn append(path: &str, data: &str) -> I64 {
    let mut file = match OpenOptions::new().append(true).create(true).open(path) {
        Ok(f) => f,
        Err(_) => return -1,
    };
    match file.write_all(data.as_bytes()) {
        Ok(()) => data.len() as i64,
        Err(_) => -1,
    }
}

/// 1 if `path` exists, else 0.
#[rtse::function(module = "fs", value = "exists")]
fn exists(path: &str) -> I64 {
    if std::path::Path::new(path).exists() { 1 } else { 0 }
}

/// 1 if `path` is a file, else 0.
#[rtse::function(module = "fs", value = "is_file")]
fn is_file(path: &str) -> I64 {
    if std::path::Path::new(path).is_file() { 1 } else { 0 }
}

/// 1 if `path` is a directory, else 0.
#[rtse::function(module = "fs", value = "is_dir")]
fn is_dir(path: &str) -> I64 {
    if std::path::Path::new(path).is_dir() { 1 } else { 0 }
}

/// File size in bytes, -1 on error.
#[rtse::function(module = "fs", value = "size")]
fn size(path: &str) -> I64 {
    match std::fs::metadata(path) {
        Ok(m) => m.len() as i64,
        Err(_) => -1,
    }
}

/// Last-modified time in ms since the UNIX epoch, -1 on error.
#[rtse::function(module = "fs", value = "modified_ms")]
fn modified_ms(path: &str) -> I64 {
    let Ok(meta) = std::fs::metadata(path) else {
        return -1;
    };
    let Ok(time) = meta.modified() else { return -1 };
    match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(dur) => dur.as_millis().min(i64::MAX as u128) as i64,
        Err(_) => -1,
    }
}

/// Creates the directory at `path` (parent must exist). 0 / -1.
#[rtse::function(module = "fs", value = "create_dir")]
fn create_dir(path: &str) -> I64 {
    match std::fs::create_dir(path) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Creates the directory and all missing parents. 0 / -1.
#[rtse::function(module = "fs", value = "create_dir_all")]
fn create_dir_all(path: &str) -> I64 {
    match std::fs::create_dir_all(path) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Removes the empty directory at `path`. 0 / -1.
#[rtse::function(module = "fs", value = "remove_dir")]
fn remove_dir(path: &str) -> I64 {
    match std::fs::remove_dir(path) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Removes the directory at `path` recursively. 0 / -1.
#[rtse::function(module = "fs", value = "remove_dir_all")]
fn remove_dir_all(path: &str) -> I64 {
    match std::fs::remove_dir_all(path) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Removes the file at `path`. 0 / -1.
#[rtse::function(module = "fs", value = "remove_file")]
fn remove_file(path: &str) -> I64 {
    match std::fs::remove_file(path) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Renames `from` to `to`. 0 / -1.
#[rtse::function(module = "fs", value = "rename")]
fn rename(from: &str, to: &str) -> I64 {
    match std::fs::rename(from, to) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Lists directory entry names (file_name only) as a Vec<i64> of string
/// handles. 0 on error.
#[rtse::function(module = "fs", value = "readdir", ret_ts = "string[]")]
fn readdir(path: &str) -> Handle {
    let Ok(iter) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut entries: Vec<i64> = Vec::new();
    for entry in iter.flatten() {
        let name = entry.file_name();
        if let Some(s) = name.to_str() {
            // Elemento como string WORD (motor novo), não handle cru.
            entries.push(rts_engine::heap::shapes::string_word(s.as_bytes()) as i64);
        }
    }
    alloc_entry(Entry::vec(entries))
}

/// Copies file contents from `from` to `to`. Bytes copied, -1 on error.
#[rtse::function(module = "fs", value = "copy")]
fn copy(from: &str, to: &str) -> I64 {
    match std::fs::copy(from, to) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

/// Função `fs.f(args)` — usada SÓ pelos 3 overloads `Bool` abaixo, que
/// deliberadamente compartilham o MESMO símbolo extern que o membro `number`
/// (`exists`/`is_file`/`is_dir`); `#[rtse::function]` sempre cunha um símbolo
/// novo por invocação, então não há como expressar essa reutilização com a
/// macro sem duplicar a implementação.
fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
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
        ret_class: None,
        pure: false,
        emit: None,
    }
}

/// Registra a namespace `fs` no motor.
pub fn register(e: &mut Engine) {
    e.ns("fs")
        .doc("Filesystem operations (std::fs): read/write, metadata, dirs, file ops.")
        .member(read_entry())
        .member(read_all_entry())
        .member({
            let mut m = func(
                "read_text",
                "__RTS_FN_NS_FS_READ_TEXT",
                Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
                "read_text(path: string): string",
                "Reads the whole file as a UTF-8 string handle. 0 on error.",
                __RTS_FN_NS_FS_READ_TEXT as *const u8,
            );
            m.aliases = vec!["readFileSync".to_string()];
            m
        })
        .member({
            let mut m = write_entry();
            m.aliases = vec!["writeFileSync".to_string()];
            m
        })
        .member(write_bytes_entry())
        .member({
            let mut m = append_entry();
            m.aliases = vec!["appendFileSync".to_string()];
            m
        })
        .member(exists_entry())
        .member(is_file_entry())
        .member(is_dir_entry())
        .member({
            let mut m = size_entry();
            m.aliases = vec!["sizeSync".to_string()];
            m
        })
        .member({
            let mut m = modified_ms_entry();
            m.aliases = vec!["mtimeMsSync".to_string()];
            m
        })
        .member(create_dir_entry())
        .member({
            let mut m = create_dir_all_entry();
            m.aliases = vec!["mkdirSync".to_string()];
            m
        })
        .member({
            let mut m = remove_dir_entry();
            m.aliases = vec!["rmdirSync".to_string()];
            m
        })
        .member(remove_dir_all_entry())
        .member({
            let mut m = remove_file_entry();
            m.aliases = vec!["rmSync".to_string(), "unlinkSync".to_string()];
            m
        })
        .member({
            let mut m = rename_entry();
            m.aliases = vec!["renameSync".to_string()];
            m
        })
        .member({
            let mut m = readdir_entry();
            m.aliases = vec!["readdirSync".to_string()];
            m
        })
        .member({
            let mut m = copy_entry();
            m.aliases = vec!["copyFileSync".to_string()];
            m
        })
        // node:fs predicate API — MESMOS símbolos `__RTS_FN_NS_FS_*`, mas
        // declarados com retorno `Bool` (não `I64`) para que o rebox produza um
        // `true`/`false` real (node:fs.existsSync etc retornam boolean estrito,
        // `=== true`). O `Bool` trafega como `i64 {0,1}` — ABI idêntica ao membro
        // numérico que compartilha o símbolo. Ergonomia node, sem mexer no
        // `fs.exists` nativo (que segue `number`).
        .member(func(
            "existsSync",
            "__rtsm_fs_exists",
            Sig::new(vec![AbiType::StrPtr], AbiType::Bool),
            "existsSync(path: string): boolean",
            "true if `path` exists, else false (node:fs).",
            __rtsm_fs_exists as *const u8,
        ))
        .member(func(
            "isFileSync",
            "__rtsm_fs_is_file",
            Sig::new(vec![AbiType::StrPtr], AbiType::Bool),
            "isFileSync(path: string): boolean",
            "true if `path` is a file, else false (node:fs extension).",
            __rtsm_fs_is_file as *const u8,
        ))
        .member(func(
            "isDirectorySync",
            "__rtsm_fs_is_dir",
            Sig::new(vec![AbiType::StrPtr], AbiType::Bool),
            "isDirectorySync(path: string): boolean",
            "true if `path` is a directory, else false (node:fs extension).",
            __rtsm_fs_is_dir as *const u8,
        ))
        .done();
}
