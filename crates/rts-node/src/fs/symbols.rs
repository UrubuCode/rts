//! node:fs — base extern "C" symbol implementations (the sync surface).

use std::fs::OpenOptions;
use std::io::Write;

use rts_engine::abi::str_abi::from_abi;
use rts_engine::heap::handles::{alloc_entry, Entry};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

/// Interns a Rust string as a GC string handle (the ABI `Handle` return).
fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// `fs.existsSync(path)` — true iff `path` exists (file, dir, or other).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_EXISTS_SYNC(path_ptr: *const u8, path_len: i64) -> i64 {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return 0;
    };
    if std::path::Path::new(path).exists() { 1 } else { 0 }
}

/// `fs.readFileSync(path)` — UTF-8 text variant. 0 on any error (missing
/// file, permission denied, invalid UTF-8 — `std::fs::read_to_string` folds
/// all three into one `Result`, matching a fail-soft ABI with no exceptions).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_READ_FILE_SYNC(path_ptr: *const u8, path_len: i64) -> u64 {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return 0;
    };
    match std::fs::read_to_string(path) {
        Ok(s) => intern(&s),
        Err(_) => 0,
    }
}

/// `fs.writeFileSync(path, data)` — creates or truncates `path` and writes
/// `data`. Errors are swallowed (void ABI has no error channel); Node throws,
/// this fails soft rather than panicking on bad input.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_WRITE_FILE_SYNC(
    path_ptr: *const u8,
    path_len: i64,
    data_ptr: *const u8,
    data_len: i64,
) {
    let (Some(path), Some(data)) =
        (unsafe { from_abi(path_ptr, path_len) }, unsafe { from_abi(data_ptr, data_len) })
    else {
        return;
    };
    let _ = std::fs::write(path, data.as_bytes());
}

/// `fs.appendFileSync(path, data)` — creates `path` if missing, appends
/// `data` otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_APPEND_FILE_SYNC(
    path_ptr: *const u8,
    path_len: i64,
    data_ptr: *const u8,
    data_len: i64,
) {
    let (Some(path), Some(data)) =
        (unsafe { from_abi(path_ptr, path_len) }, unsafe { from_abi(data_ptr, data_len) })
    else {
        return;
    };
    if let Ok(mut file) = OpenOptions::new().append(true).create(true).open(path) {
        let _ = file.write_all(data.as_bytes());
    }
}

/// `fs.mkdirSync(path)` — **non-recursive**: the parent must already exist
/// (mirrors `std::fs::create_dir`). Node's `{ recursive: true }` option is
/// deferred (needs an options object).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_MKDIR_SYNC(path_ptr: *const u8, path_len: i64) {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return;
    };
    let _ = std::fs::create_dir(path);
}

/// `fs.rmdirSync(path)` — removes an *empty* directory. Node's recursive
/// `{ recursive: true }` form (deprecated in Node itself in favor of
/// `rmSync`) is deferred (needs an options object).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_RMDIR_SYNC(path_ptr: *const u8, path_len: i64) {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return;
    };
    let _ = std::fs::remove_dir(path);
}

/// `fs.unlinkSync(path)` — removes a file.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_UNLINK_SYNC(path_ptr: *const u8, path_len: i64) {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return;
    };
    let _ = std::fs::remove_file(path);
}

/// `fs.renameSync(oldPath, newPath)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_RENAME_SYNC(
    old_ptr: *const u8,
    old_len: i64,
    new_ptr: *const u8,
    new_len: i64,
) {
    let (Some(old_path), Some(new_path)) =
        (unsafe { from_abi(old_ptr, old_len) }, unsafe { from_abi(new_ptr, new_len) })
    else {
        return;
    };
    let _ = std::fs::rename(old_path, new_path);
}

/// `fs.copyFileSync(src, dest)` — copies file contents (and permission bits,
/// via `std::fs::copy`), overwriting `dest`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_COPY_FILE_SYNC(
    src_ptr: *const u8,
    src_len: i64,
    dest_ptr: *const u8,
    dest_len: i64,
) {
    let (Some(src), Some(dest)) =
        (unsafe { from_abi(src_ptr, src_len) }, unsafe { from_abi(dest_ptr, dest_len) })
    else {
        return;
    };
    let _ = std::fs::copy(src, dest);
}

/// `fs.readdirSync(path)` — entry names only (`file_name()`, matching Node's
/// default with no `withFileTypes`/`recursive` options). 0 on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_READDIR_SYNC(path_ptr: *const u8, path_len: i64) -> u64 {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return 0;
    };
    let Ok(iter) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut entries: Vec<i64> = Vec::new();
    for entry in iter.flatten() {
        let name = entry.file_name();
        if let Some(s) = name.to_str() {
            // Element as a string WORD (new engine), not a raw handle.
            entries.push(rts_engine::heap::shapes::string_word(s.as_bytes()) as i64);
        }
    }
    alloc_entry(Entry::Vec(Box::new(entries)))
}

/// `fs.realpathSync(path)` — resolves symlinks/`.`/`..` against the real
/// filesystem (`std::fs::canonicalize`). 0 on error (missing path, or a
/// resolved path that is not valid UTF-8).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_REALPATH_SYNC(path_ptr: *const u8, path_len: i64) -> u64 {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return 0;
    };
    match std::fs::canonicalize(path) {
        Ok(resolved) => resolved.to_str().map(intern).unwrap_or(0),
        Err(_) => 0,
    }
}
