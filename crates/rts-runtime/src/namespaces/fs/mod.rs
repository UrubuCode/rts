//! `fs` namespace — filesystem operations backed by `std::fs`.
//!
//! Path args arrive as `Str` (reconstructed `&str`; `&str: AsRef<Path>`).
//! Byte buffers travel as a `U64` pointer cast to `*mut`/`*const u8`. Status
//! functions return `-1` on error (`on_null = -1`); `exists`/`is_*` return 0/1.
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};

use rts_engine::abi::ty::{Handle, I64, U64};
use rts_macro::rts_namespace;

use crate::namespaces::gc::handles::{Entry, alloc_entry};
use crate::namespaces::gc::string_pool::__RTS_FN_NS_GC_STRING_NEW;

/// Filesystem operations (std::fs): read/write, metadata, dirs, file ops.
#[rts_namespace(fs)]
impl FsNs {
    /// Reads up to `bufLen` bytes from `path` into the buffer. Count, 0 on EOF, -1 on error.
    #[rts_fn(ts = "read(path: string, bufPtr: number, bufLen: number): number", on_null = -1)]
    pub fn read(path: Str, buf_ptr: U64, buf_len: I64) -> I64 {
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
    #[rts_fn(ts = "read_all(path: string, bufPtr: number, bufLen: number): number", on_null = -1)]
    pub fn read_all(path: Str, buf_ptr: U64, buf_len: I64) -> I64 {
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
    #[rts_fn(ts = "read_text(path: string): string")]
    pub fn read_text(path: Str) -> Handle {
        match std::fs::read(path) {
            Ok(b) => alloc_entry(Entry::String(b)),
            Err(_) => 0,
        }
    }

    /// Writes `data` to `path` (truncating). Bytes written, -1 on error.
    #[rts_fn(on_null = -1)]
    pub fn write(path: Str, data: Str) -> I64 {
        match std::fs::write(path, data.as_bytes()) {
            Ok(()) => data.len() as i64,
            Err(_) => -1,
        }
    }

    /// Writes raw buffer bytes to `path` (truncating). Bytes written, -1 on error.
    #[rts_fn(ts = "write_bytes(path: string, bufPtr: number, len: number): number", on_null = -1)]
    pub fn write_bytes(path: Str, buf_ptr: U64, len: I64) -> I64 {
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
    #[rts_fn(on_null = -1)]
    pub fn append(path: Str, data: Str) -> I64 {
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
    #[rts_fn]
    pub fn exists(path: Str) -> I64 {
        if std::path::Path::new(path).exists() {
            1
        } else {
            0
        }
    }

    /// 1 if `path` is a file, else 0.
    #[rts_fn]
    pub fn is_file(path: Str) -> I64 {
        if std::path::Path::new(path).is_file() {
            1
        } else {
            0
        }
    }

    /// 1 if `path` is a directory, else 0.
    #[rts_fn]
    pub fn is_dir(path: Str) -> I64 {
        if std::path::Path::new(path).is_dir() {
            1
        } else {
            0
        }
    }

    /// File size in bytes, -1 on error.
    #[rts_fn(on_null = -1)]
    pub fn size(path: Str) -> I64 {
        match std::fs::metadata(path) {
            Ok(m) => m.len() as i64,
            Err(_) => -1,
        }
    }

    /// Last-modified time in ms since the UNIX epoch, -1 on error.
    #[rts_fn(on_null = -1)]
    pub fn modified_ms(path: Str) -> I64 {
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
    #[rts_fn(on_null = -1)]
    pub fn create_dir(path: Str) -> I64 {
        match std::fs::create_dir(path) {
            Ok(()) => 0,
            Err(_) => -1,
        }
    }

    /// Creates the directory and all missing parents. 0 / -1.
    #[rts_fn(on_null = -1)]
    pub fn create_dir_all(path: Str) -> I64 {
        match std::fs::create_dir_all(path) {
            Ok(()) => 0,
            Err(_) => -1,
        }
    }

    /// Removes the empty directory at `path`. 0 / -1.
    #[rts_fn(on_null = -1)]
    pub fn remove_dir(path: Str) -> I64 {
        match std::fs::remove_dir(path) {
            Ok(()) => 0,
            Err(_) => -1,
        }
    }

    /// Removes the directory at `path` recursively. 0 / -1.
    #[rts_fn(on_null = -1)]
    pub fn remove_dir_all(path: Str) -> I64 {
        match std::fs::remove_dir_all(path) {
            Ok(()) => 0,
            Err(_) => -1,
        }
    }

    /// Removes the file at `path`. 0 / -1.
    #[rts_fn(on_null = -1)]
    pub fn remove_file(path: Str) -> I64 {
        match std::fs::remove_file(path) {
            Ok(()) => 0,
            Err(_) => -1,
        }
    }

    /// Renames `from` to `to`. 0 / -1.
    #[rts_fn(on_null = -1)]
    pub fn rename(from: Str, to: Str) -> I64 {
        match std::fs::rename(from, to) {
            Ok(()) => 0,
            Err(_) => -1,
        }
    }

    /// Lists directory entry names (file_name only) as a Vec<i64> of string
    /// handles. 0 on error.
    #[rts_fn]
    pub fn readdir(path: Str) -> Handle {
        let Ok(iter) = std::fs::read_dir(path) else {
            return 0;
        };
        let mut entries: Vec<i64> = Vec::new();
        for entry in iter.flatten() {
            let name = entry.file_name();
            if let Some(s) = name.to_str() {
                let h = __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64);
                entries.push(h as i64);
            }
        }
        alloc_entry(Entry::Vec(Box::new(entries)))
    }

    /// Copies file contents from `from` to `to`. Bytes copied, -1 on error.
    #[rts_fn(on_null = -1)]
    pub fn copy(from: Str, to: Str) -> I64 {
        match std::fs::copy(from, to) {
            Ok(n) => n as i64,
            Err(_) => -1,
        }
    }
}
