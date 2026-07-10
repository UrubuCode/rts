//! `node:fs/promises` — the Promise-returning fs surface, real `std::fs`
//! backed. Registered as its OWN namespace (`node:fs/promises`, distinct
//! from `node:fs`) by `mod.rs`.
//!
//! Every member here ALWAYS settles a Promise — unlike the sync `void`
//! members in `symbols.rs` (which have no error channel and silently
//! swallow a failed syscall, see the doc-comments there), a rejected
//! Promise here carries the REAL `std::io::Error` message, so `.catch()`
//! observes a genuine failure.
//!
//! **Settled-value convention** — mirrors the pattern already live in
//! production rather than inventing a new one: `Blob.text(): Promise<string>`
//! (`rts-std/src/globals/blob/mod.rs::__RTS_FN_GL_BLOB_TEXT`) settles with
//! the RAW string handle (`alloc_entry(Entry::String(..))`), NOT a boxed
//! PolyValue word; `ReadableStream`'s `Promise<void>` members
//! (`rts-std/src/globals/readable_stream/instance.rs`) settle with the
//! legacy raw-i64 `UNDEFINED = i64::MIN + 2` sentinel. So here: the `value`
//! passed to `new_resolved`/`new_rejected` is exactly what a plain
//! (non-Promise) member of that same `AbiType` would return — a raw
//! GC string/array/object handle for `Handle`-typed results (`readFile` →
//! string handle, `readdir`/`stat` → the same handle their sync
//! counterparts build), or [`UNDEFINED`] for `void`. The engine's `Promise<T>`
//! return-class typing (`ts_return_class` in
//! `rts-codegen-new/src/front/run/registry.rs`) then reads the settled value
//! with the SAME decoder it already uses for a direct `T`-typed return — no
//! new convention introduced.
//!
//! `rts-node` links only against `rts-engine` (see `Cargo.toml` — no
//! `rts-std` Cargo dependency), so `promise.new_resolved`/`new_rejected` are
//! reached the same way `__RTS_FN_NS_GC_STRING_NEW` already is throughout
//! this crate: a raw `unsafe extern "C"` declaration resolved by the FINAL
//! link (the whole-program binary links `rts-node` alongside
//! `rts-std`/`rts-runtime`, which is where these two symbols are actually
//! defined — see `rts-std/src/promise/mod.rs::__RTS_FN_NS_PROMISE_NEW_RESOLVED/
//! NEW_REJECTED`, confirmed to take/return exactly `(value: i64) -> u64` /
//! `(error: i64) -> u64` before wiring this up).
//!
//! Members: `readFile`/`writeFile`/`appendFile`/`mkdir`/`rmdir`/`unlink`/
//! `rename`/`copyFile`/`readdir`/`stat`/`access`, all `Promise<...>`.
//! **Deferred** (same reasons as `node:fs`'s sync surface, see
//! `symbols.rs`/`mod.rs`): recursive `mkdir`/`rmdir`/`rm` options objects,
//! `lstat`, `realpath`, `readlink`, `truncate`, `mkdtemp`, `watch`, streams,
//! `Dirent`/`withFileTypes`, any non-UTF8 encoding.

use std::fs::OpenOptions;
use std::io::Write;

use rts_engine::abi::str_abi::from_abi;
use rts_engine::heap::handles::{alloc_entry, Entry};
use rts_engine::heap::shapes::string_word;

use super::symbols;

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
    fn __RTS_FN_NS_PROMISE_NEW_RESOLVED(value: i64) -> u64;
    fn __RTS_FN_NS_PROMISE_NEW_REJECTED(error: i64) -> u64;
}

/// The legacy raw-i64 `undefined` sentinel (mirrors
/// `readable_stream/instance.rs::UNDEFINED` / `heap::shapes::RAW_UNDEFINED`)
/// — the settled value of every `Promise<void>` member here.
const UNDEFINED: i64 = i64::MIN + 2;

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

fn resolved(value: i64) -> u64 {
    unsafe { __RTS_FN_NS_PROMISE_NEW_RESOLVED(value) }
}

fn resolved_undefined() -> u64 {
    resolved(UNDEFINED)
}

/// Decodes a `(ptr, len)` ABI string arg, or settles a REJECTED Promise
/// describing the bad argument. Unlike the sync `void` ops (no error
/// channel, silently no-op on a bad arg), a Promise-returning call always
/// has somewhere to report this.
fn decode_or_reject<'a>(ptr: *const u8, len: i64, arg_name: &str) -> Result<&'a str, u64> {
    match unsafe { from_abi(ptr, len) } {
        Some(s) => Ok(s),
        None => Err(reject_msg(&format!("ERR_INVALID_ARG_VALUE: invalid {arg_name}"))),
    }
}

fn reject_msg(message: &str) -> u64 {
    let h = intern(message);
    unsafe { __RTS_FN_NS_PROMISE_NEW_REJECTED(h as i64) }
}

macro_rules! path_or_reject {
    ($ptr:expr, $len:expr) => {
        match decode_or_reject($ptr, $len, "path") {
            Ok(p) => p,
            Err(rej) => return rej,
        }
    };
}

/// `fsPromises.readFile(path)` — resolves with the UTF-8 text content (a raw
/// string handle, same as `readFileSync`'s `Handle` return), rejects with
/// the real I/O error otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_PROMISES_READ_FILE(path_ptr: *const u8, path_len: i64) -> u64 {
    let path = path_or_reject!(path_ptr, path_len);
    match std::fs::read_to_string(path) {
        Ok(s) => resolved(intern(&s) as i64),
        Err(e) => reject_msg(&format!("{path}: {e}")),
    }
}

/// `fsPromises.writeFile(path, data)` — creates or truncates `path`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_PROMISES_WRITE_FILE(
    path_ptr: *const u8,
    path_len: i64,
    data_ptr: *const u8,
    data_len: i64,
) -> u64 {
    let path = path_or_reject!(path_ptr, path_len);
    let data = match decode_or_reject(data_ptr, data_len, "data") {
        Ok(d) => d,
        Err(rej) => return rej,
    };
    match std::fs::write(path, data.as_bytes()) {
        Ok(()) => resolved_undefined(),
        Err(e) => reject_msg(&format!("{path}: {e}")),
    }
}

/// `fsPromises.appendFile(path, data)` — creates `path` if missing, appends
/// otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_PROMISES_APPEND_FILE(
    path_ptr: *const u8,
    path_len: i64,
    data_ptr: *const u8,
    data_len: i64,
) -> u64 {
    let path = path_or_reject!(path_ptr, path_len);
    let data = match decode_or_reject(data_ptr, data_len, "data") {
        Ok(d) => d,
        Err(rej) => return rej,
    };
    match OpenOptions::new().append(true).create(true).open(path) {
        Ok(mut file) => match file.write_all(data.as_bytes()) {
            Ok(()) => resolved_undefined(),
            Err(e) => reject_msg(&format!("{path}: {e}")),
        },
        Err(e) => reject_msg(&format!("{path}: {e}")),
    }
}

/// `fsPromises.mkdir(path)` — non-recursive (parent must already exist);
/// the `{ recursive: true }` options form is deferred (see `node:fs`'s
/// `mkdirpSync` for the recursive SYNC variant; no async counterpart yet).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_PROMISES_MKDIR(path_ptr: *const u8, path_len: i64) -> u64 {
    let path = path_or_reject!(path_ptr, path_len);
    match std::fs::create_dir(path) {
        Ok(()) => resolved_undefined(),
        Err(e) => reject_msg(&format!("{path}: {e}")),
    }
}

/// `fsPromises.rmdir(path)` — removes an *empty* directory.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_PROMISES_RMDIR(path_ptr: *const u8, path_len: i64) -> u64 {
    let path = path_or_reject!(path_ptr, path_len);
    match std::fs::remove_dir(path) {
        Ok(()) => resolved_undefined(),
        Err(e) => reject_msg(&format!("{path}: {e}")),
    }
}

/// `fsPromises.unlink(path)` — removes a file.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_PROMISES_UNLINK(path_ptr: *const u8, path_len: i64) -> u64 {
    let path = path_or_reject!(path_ptr, path_len);
    match std::fs::remove_file(path) {
        Ok(()) => resolved_undefined(),
        Err(e) => reject_msg(&format!("{path}: {e}")),
    }
}

/// `fsPromises.rename(oldPath, newPath)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_PROMISES_RENAME(
    old_ptr: *const u8,
    old_len: i64,
    new_ptr: *const u8,
    new_len: i64,
) -> u64 {
    let old_path = match decode_or_reject(old_ptr, old_len, "oldPath") {
        Ok(p) => p,
        Err(rej) => return rej,
    };
    let new_path = match decode_or_reject(new_ptr, new_len, "newPath") {
        Ok(p) => p,
        Err(rej) => return rej,
    };
    match std::fs::rename(old_path, new_path) {
        Ok(()) => resolved_undefined(),
        Err(e) => reject_msg(&format!("{old_path} -> {new_path}: {e}")),
    }
}

/// `fsPromises.copyFile(src, dest)` — copies contents + permission bits,
/// overwriting `dest`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_PROMISES_COPY_FILE(
    src_ptr: *const u8,
    src_len: i64,
    dest_ptr: *const u8,
    dest_len: i64,
) -> u64 {
    let src = match decode_or_reject(src_ptr, src_len, "src") {
        Ok(p) => p,
        Err(rej) => return rej,
    };
    let dest = match decode_or_reject(dest_ptr, dest_len, "dest") {
        Ok(p) => p,
        Err(rej) => return rej,
    };
    match std::fs::copy(src, dest) {
        Ok(_) => resolved_undefined(),
        Err(e) => reject_msg(&format!("{src} -> {dest}: {e}")),
    }
}

/// `fsPromises.readdir(path)` — entry names only (no `withFileTypes`), same
/// element convention as `readdirSync` (`string[]`, elements as PolyValue
/// string WORDS inside the Vec — the Vec HANDLE ITSELF is the raw settled
/// value, matching `readdirSync`'s `Handle` return).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_PROMISES_READDIR(path_ptr: *const u8, path_len: i64) -> u64 {
    let path = path_or_reject!(path_ptr, path_len);
    let iter = match std::fs::read_dir(path) {
        Ok(it) => it,
        Err(e) => return reject_msg(&format!("{path}: {e}")),
    };
    let mut entries: Vec<i64> = Vec::new();
    for entry in iter.flatten() {
        if let Some(s) = entry.file_name().to_str() {
            entries.push(string_word(s.as_bytes()) as i64);
        }
    }
    let h = alloc_entry(Entry::Vec(Box::new(entries)));
    resolved(h as i64)
}

/// `fsPromises.stat(path)` — follows symlinks; same `Stats`-shaped object
/// as `statSync` (reuses `symbols::build_stats_object`, no duplicated
/// object-construction logic).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_PROMISES_STAT(path_ptr: *const u8, path_len: i64) -> u64 {
    let path = path_or_reject!(path_ptr, path_len);
    match std::fs::metadata(path) {
        Ok(meta) => resolved(symbols::build_stats_object(&meta) as i64),
        Err(e) => reject_msg(&format!("{path}: {e}")),
    }
}

/// `fsPromises.access(path)` — resolves iff the real `std::fs::metadata`
/// probe succeeds (existence + readable by this process), rejects with the
/// real I/O error otherwise — unlike `accessSync` (`void` ABI, no error
/// channel), a Promise genuinely reports pass/fail here.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_PROMISES_ACCESS(path_ptr: *const u8, path_len: i64) -> u64 {
    let path = path_or_reject!(path_ptr, path_len);
    match std::fs::metadata(path) {
        Ok(_) => resolved_undefined(),
        Err(e) => reject_msg(&format!("{path}: {e}")),
    }
}
