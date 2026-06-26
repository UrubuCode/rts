//! `fs` namespace — filesystem operations backed by `std::fs`.
//!
//! Path args arrive as strings (reconstructed `&str`; `&str: AsRef<Path>`).
//! Byte buffers travel as a `U64` pointer cast to `*mut`/`*const u8`. Status
//! functions return `-1` on error (`on_null = -1`); `exists`/`is_*` return 0/1.
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem/runtime).

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};

use rts_engine::abi::ty::{Handle, I64, U64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{Entry, alloc_entry};

// `string_pool` (intern de strings GC) fica no backend (rts-runtime collector);
// referenciado por símbolo (link cross-crate) p/ o `fs` poder viver no rts-std.
unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

/// Reads up to `bufLen` bytes from `path` into the buffer. Count, 0 on EOF, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_READ(
    path_ptr: *const u8,
    path_len: i64,
    buf_ptr: U64,
    buf_len: I64,
) -> I64 {
    let path = match unsafe { rts_engine::abi::str_abi::from_abi(path_ptr, path_len) } {
        Some(s) => s,
        None => return -1,
    };
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_READ_ALL(
    path_ptr: *const u8,
    path_len: i64,
    buf_ptr: U64,
    buf_len: I64,
) -> I64 {
    let path = match unsafe { rts_engine::abi::str_abi::from_abi(path_ptr, path_len) } {
        Some(s) => s,
        None => return -1,
    };
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_READ_TEXT(path_ptr: *const u8, path_len: i64) -> Handle {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_WRITE(
    path_ptr: *const u8,
    path_len: i64,
    data_ptr: *const u8,
    data_len: i64,
) -> I64 {
    let path = match unsafe { rts_engine::abi::str_abi::from_abi(path_ptr, path_len) } {
        Some(s) => s,
        None => return -1,
    };
    let data = match unsafe { rts_engine::abi::str_abi::from_abi(data_ptr, data_len) } {
        Some(s) => s,
        None => return -1,
    };
    match std::fs::write(path, data.as_bytes()) {
        Ok(()) => data.len() as i64,
        Err(_) => -1,
    }
}

/// Writes raw buffer bytes to `path` (truncating). Bytes written, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_WRITE_BYTES(
    path_ptr: *const u8,
    path_len: i64,
    buf_ptr: U64,
    len: I64,
) -> I64 {
    let path = match unsafe { rts_engine::abi::str_abi::from_abi(path_ptr, path_len) } {
        Some(s) => s,
        None => return -1,
    };
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_APPEND(
    path_ptr: *const u8,
    path_len: i64,
    data_ptr: *const u8,
    data_len: i64,
) -> I64 {
    let path = match unsafe { rts_engine::abi::str_abi::from_abi(path_ptr, path_len) } {
        Some(s) => s,
        None => return -1,
    };
    let data = match unsafe { rts_engine::abi::str_abi::from_abi(data_ptr, data_len) } {
        Some(s) => s,
        None => return -1,
    };
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_EXISTS(path_ptr: *const u8, path_len: i64) -> I64 {
    let path = match unsafe { rts_engine::abi::str_abi::from_abi(path_ptr, path_len) } {
        Some(s) => s,
        None => return 0,
    };
    if std::path::Path::new(path).exists() {
        1
    } else {
        0
    }
}

/// 1 if `path` is a file, else 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_IS_FILE(path_ptr: *const u8, path_len: i64) -> I64 {
    let path = match unsafe { rts_engine::abi::str_abi::from_abi(path_ptr, path_len) } {
        Some(s) => s,
        None => return 0,
    };
    if std::path::Path::new(path).is_file() {
        1
    } else {
        0
    }
}

/// 1 if `path` is a directory, else 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_IS_DIR(path_ptr: *const u8, path_len: i64) -> I64 {
    let path = match unsafe { rts_engine::abi::str_abi::from_abi(path_ptr, path_len) } {
        Some(s) => s,
        None => return 0,
    };
    if std::path::Path::new(path).is_dir() {
        1
    } else {
        0
    }
}

/// File size in bytes, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_SIZE(path_ptr: *const u8, path_len: i64) -> I64 {
    let path = match unsafe { rts_engine::abi::str_abi::from_abi(path_ptr, path_len) } {
        Some(s) => s,
        None => return -1,
    };
    match std::fs::metadata(path) {
        Ok(m) => m.len() as i64,
        Err(_) => -1,
    }
}

/// Last-modified time in ms since the UNIX epoch, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_MODIFIED_MS(path_ptr: *const u8, path_len: i64) -> I64 {
    let path = match unsafe { rts_engine::abi::str_abi::from_abi(path_ptr, path_len) } {
        Some(s) => s,
        None => return -1,
    };
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_CREATE_DIR(path_ptr: *const u8, path_len: i64) -> I64 {
    let path = match unsafe { rts_engine::abi::str_abi::from_abi(path_ptr, path_len) } {
        Some(s) => s,
        None => return -1,
    };
    match std::fs::create_dir(path) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Creates the directory and all missing parents. 0 / -1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_CREATE_DIR_ALL(path_ptr: *const u8, path_len: i64) -> I64 {
    let path = match unsafe { rts_engine::abi::str_abi::from_abi(path_ptr, path_len) } {
        Some(s) => s,
        None => return -1,
    };
    match std::fs::create_dir_all(path) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Removes the empty directory at `path`. 0 / -1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_REMOVE_DIR(path_ptr: *const u8, path_len: i64) -> I64 {
    let path = match unsafe { rts_engine::abi::str_abi::from_abi(path_ptr, path_len) } {
        Some(s) => s,
        None => return -1,
    };
    match std::fs::remove_dir(path) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Removes the directory at `path` recursively. 0 / -1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_REMOVE_DIR_ALL(path_ptr: *const u8, path_len: i64) -> I64 {
    let path = match unsafe { rts_engine::abi::str_abi::from_abi(path_ptr, path_len) } {
        Some(s) => s,
        None => return -1,
    };
    match std::fs::remove_dir_all(path) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Removes the file at `path`. 0 / -1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_REMOVE_FILE(path_ptr: *const u8, path_len: i64) -> I64 {
    let path = match unsafe { rts_engine::abi::str_abi::from_abi(path_ptr, path_len) } {
        Some(s) => s,
        None => return -1,
    };
    match std::fs::remove_file(path) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Renames `from` to `to`. 0 / -1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_RENAME(
    from_ptr: *const u8,
    from_len: i64,
    to_ptr: *const u8,
    to_len: i64,
) -> I64 {
    let from = match unsafe { rts_engine::abi::str_abi::from_abi(from_ptr, from_len) } {
        Some(s) => s,
        None => return -1,
    };
    let to = match unsafe { rts_engine::abi::str_abi::from_abi(to_ptr, to_len) } {
        Some(s) => s,
        None => return -1,
    };
    match std::fs::rename(from, to) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Lists directory entry names (file_name only) as a Vec<i64> of string
/// handles. 0 on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_READDIR(path_ptr: *const u8, path_len: i64) -> Handle {
    let path = match unsafe { rts_engine::abi::str_abi::from_abi(path_ptr, path_len) } {
        Some(s) => s,
        None => return 0,
    };
    let Ok(iter) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut entries: Vec<i64> = Vec::new();
    for entry in iter.flatten() {
        let name = entry.file_name();
        if let Some(s) = name.to_str() {
            let h = unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) };
            entries.push(h as i64);
        }
    }
    alloc_entry(Entry::Vec(Box::new(entries)))
}

/// Copies file contents from `from` to `to`. Bytes copied, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_COPY(
    from_ptr: *const u8,
    from_len: i64,
    to_ptr: *const u8,
    to_len: i64,
) -> I64 {
    let from = match unsafe { rts_engine::abi::str_abi::from_abi(from_ptr, from_len) } {
        Some(s) => s,
        None => return -1,
    };
    let to = match unsafe { rts_engine::abi::str_abi::from_abi(to_ptr, to_len) } {
        Some(s) => s,
        None => return -1,
    };
    match std::fs::copy(from, to) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

/// Função `fs.f(args)`.
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
        pure: false,
        intrinsic: None,
    }
}

/// Anexa `aliases` (nomes alternativos) a um `Member` já construído. Usado para
/// expor a API node:fs (`writeFileSync`, `existsSync`, …) como apelidos dos
/// membros nativos `fs.*` — resolução por dado (`Member::matches_name`), o motor
/// não nomeia nada. Espelha o mapa de `rts-node/src/fs`.
fn with_aliases(mut m: Member, aliases: &[&str]) -> Member {
    m.aliases = aliases.iter().map(|s| s.to_string()).collect();
    m
}

/// Registra a namespace `fs` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("fs")
        .doc("Filesystem operations (std::fs): read/write, metadata, dirs, file ops.")
        .member(func(
            "read",
            "__RTS_FN_NS_FS_READ",
            Sig::new(vec![AbiType::StrPtr, AbiType::U64, AbiType::I64], AbiType::I64),
            "read(path: string, bufPtr: number, bufLen: number): number",
            "Reads up to `bufLen` bytes from `path` into the buffer. Count, 0 on EOF, -1 on error.",
            __RTS_FN_NS_FS_READ as *const u8,
        ))
        .member(func(
            "read_all",
            "__RTS_FN_NS_FS_READ_ALL",
            Sig::new(vec![AbiType::StrPtr, AbiType::U64, AbiType::I64], AbiType::I64),
            "read_all(path: string, bufPtr: number, bufLen: number): number",
            "Reads the whole file into the buffer (truncating to `bufLen`). Bytes written, -1 on error.",
            __RTS_FN_NS_FS_READ_ALL as *const u8,
        ))
        .member(with_aliases(
            func(
                "read_text",
                "__RTS_FN_NS_FS_READ_TEXT",
                Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
                "read_text(path: string): string",
                "Reads the whole file as a UTF-8 string handle. 0 on error.",
                __RTS_FN_NS_FS_READ_TEXT as *const u8,
            ),
            &["readFileSync"],
        ))
        .member(with_aliases(
            func(
                "write",
                "__RTS_FN_NS_FS_WRITE",
                Sig::new(vec![AbiType::StrPtr, AbiType::StrPtr], AbiType::I64),
                "write(path: string, data: string): number",
                "Writes `data` to `path` (truncating). Bytes written, -1 on error.",
                __RTS_FN_NS_FS_WRITE as *const u8,
            ),
            &["writeFileSync"],
        ))
        .member(func(
            "write_bytes",
            "__RTS_FN_NS_FS_WRITE_BYTES",
            Sig::new(vec![AbiType::StrPtr, AbiType::U64, AbiType::I64], AbiType::I64),
            "write_bytes(path: string, bufPtr: number, len: number): number",
            "Writes raw buffer bytes to `path` (truncating). Bytes written, -1 on error.",
            __RTS_FN_NS_FS_WRITE_BYTES as *const u8,
        ))
        .member(with_aliases(
            func(
                "append",
                "__RTS_FN_NS_FS_APPEND",
                Sig::new(vec![AbiType::StrPtr, AbiType::StrPtr], AbiType::I64),
                "append(path: string, data: string): number",
                "Appends `data` to `path` (creating it if missing). Bytes written, -1 on error.",
                __RTS_FN_NS_FS_APPEND as *const u8,
            ),
            &["appendFileSync"],
        ))
        .member(func(
            "exists",
            "__RTS_FN_NS_FS_EXISTS",
            Sig::new(vec![AbiType::StrPtr], AbiType::I64),
            "exists(path: string): number",
            "1 if `path` exists, else 0.",
            __RTS_FN_NS_FS_EXISTS as *const u8,
        ))
        .member(func(
            "is_file",
            "__RTS_FN_NS_FS_IS_FILE",
            Sig::new(vec![AbiType::StrPtr], AbiType::I64),
            "is_file(path: string): number",
            "1 if `path` is a file, else 0.",
            __RTS_FN_NS_FS_IS_FILE as *const u8,
        ))
        .member(func(
            "is_dir",
            "__RTS_FN_NS_FS_IS_DIR",
            Sig::new(vec![AbiType::StrPtr], AbiType::I64),
            "is_dir(path: string): number",
            "1 if `path` is a directory, else 0.",
            __RTS_FN_NS_FS_IS_DIR as *const u8,
        ))
        .member(with_aliases(
            func(
                "size",
                "__RTS_FN_NS_FS_SIZE",
                Sig::new(vec![AbiType::StrPtr], AbiType::I64),
                "size(path: string): number",
                "File size in bytes, -1 on error.",
                __RTS_FN_NS_FS_SIZE as *const u8,
            ),
            &["sizeSync"],
        ))
        .member(with_aliases(
            func(
                "modified_ms",
                "__RTS_FN_NS_FS_MODIFIED_MS",
                Sig::new(vec![AbiType::StrPtr], AbiType::I64),
                "modified_ms(path: string): number",
                "Last-modified time in ms since the UNIX epoch, -1 on error.",
                __RTS_FN_NS_FS_MODIFIED_MS as *const u8,
            ),
            &["mtimeMsSync"],
        ))
        .member(func(
            "create_dir",
            "__RTS_FN_NS_FS_CREATE_DIR",
            Sig::new(vec![AbiType::StrPtr], AbiType::I64),
            "create_dir(path: string): number",
            "Creates the directory at `path` (parent must exist). 0 / -1.",
            __RTS_FN_NS_FS_CREATE_DIR as *const u8,
        ))
        .member(with_aliases(
            func(
                "create_dir_all",
                "__RTS_FN_NS_FS_CREATE_DIR_ALL",
                Sig::new(vec![AbiType::StrPtr], AbiType::I64),
                "create_dir_all(path: string): number",
                "Creates the directory and all missing parents. 0 / -1.",
                __RTS_FN_NS_FS_CREATE_DIR_ALL as *const u8,
            ),
            &["mkdirSync"],
        ))
        .member(with_aliases(
            func(
                "remove_dir",
                "__RTS_FN_NS_FS_REMOVE_DIR",
                Sig::new(vec![AbiType::StrPtr], AbiType::I64),
                "remove_dir(path: string): number",
                "Removes the empty directory at `path`. 0 / -1.",
                __RTS_FN_NS_FS_REMOVE_DIR as *const u8,
            ),
            &["rmdirSync"],
        ))
        .member(func(
            "remove_dir_all",
            "__RTS_FN_NS_FS_REMOVE_DIR_ALL",
            Sig::new(vec![AbiType::StrPtr], AbiType::I64),
            "remove_dir_all(path: string): number",
            "Removes the directory at `path` recursively. 0 / -1.",
            __RTS_FN_NS_FS_REMOVE_DIR_ALL as *const u8,
        ))
        .member(with_aliases(
            func(
                "remove_file",
                "__RTS_FN_NS_FS_REMOVE_FILE",
                Sig::new(vec![AbiType::StrPtr], AbiType::I64),
                "remove_file(path: string): number",
                "Removes the file at `path`. 0 / -1.",
                __RTS_FN_NS_FS_REMOVE_FILE as *const u8,
            ),
            &["rmSync", "unlinkSync"],
        ))
        .member(with_aliases(
            func(
                "rename",
                "__RTS_FN_NS_FS_RENAME",
                Sig::new(vec![AbiType::StrPtr, AbiType::StrPtr], AbiType::I64),
                "rename(from: string, to: string): number",
                "Renames `from` to `to`. 0 / -1.",
                __RTS_FN_NS_FS_RENAME as *const u8,
            ),
            &["renameSync"],
        ))
        .member(with_aliases(
            func(
                "readdir",
                "__RTS_FN_NS_FS_READDIR",
                Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
                "readdir(path: string): number",
                "Lists directory entry names (file_name only) as a Vec<i64> of string\nhandles. 0 on error.",
                __RTS_FN_NS_FS_READDIR as *const u8,
            ),
            &["readdirSync"],
        ))
        .member(with_aliases(
            func(
                "copy",
                "__RTS_FN_NS_FS_COPY",
                Sig::new(vec![AbiType::StrPtr, AbiType::StrPtr], AbiType::I64),
                "copy(from: string, to: string): number",
                "Copies file contents from `from` to `to`. Bytes copied, -1 on error.",
                __RTS_FN_NS_FS_COPY as *const u8,
            ),
            &["copyFileSync"],
        ))
        // node:fs predicate API — MESMOS símbolos `__RTS_FN_NS_FS_*`, mas
        // declarados com retorno `Bool` (não `I64`) para que o rebox produza um
        // `true`/`false` real (node:fs.existsSync etc retornam boolean estrito,
        // `=== true`). O `Bool` trafega como `i64 {0,1}` — ABI idêntica ao membro
        // numérico que compartilha o símbolo. Ergonomia node, sem mexer no
        // `fs.exists` nativo (que segue `number`).
        .member(func(
            "existsSync",
            "__RTS_FN_NS_FS_EXISTS",
            Sig::new(vec![AbiType::StrPtr], AbiType::Bool),
            "existsSync(path: string): boolean",
            "true if `path` exists, else false (node:fs).",
            __RTS_FN_NS_FS_EXISTS as *const u8,
        ))
        .member(func(
            "isFileSync",
            "__RTS_FN_NS_FS_IS_FILE",
            Sig::new(vec![AbiType::StrPtr], AbiType::Bool),
            "isFileSync(path: string): boolean",
            "true if `path` is a file, else false (node:fs extension).",
            __RTS_FN_NS_FS_IS_FILE as *const u8,
        ))
        .member(func(
            "isDirectorySync",
            "__RTS_FN_NS_FS_IS_DIR",
            Sig::new(vec![AbiType::StrPtr], AbiType::Bool),
            "isDirectorySync(path: string): boolean",
            "true if `path` is a directory, else false (node:fs extension).",
            __RTS_FN_NS_FS_IS_DIR as *const u8,
        ))
        .done();
}
