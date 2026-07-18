//! node:fs — the `FileHandle` returned by `fs.promises.open(path, flags)`. An
//! object-backed Registry class (`__rts_class = "FileHandle"`) wrapping an fd in
//! the shared `fd` table. Its methods return already-settled Promises (the work
//! is synchronous, #207): `readFile`/`writeFile`/`stat`/`truncate`/`sync`/
//! `datasync`/`chmod`/`close`. `open` itself resolves with the `FileHandle`.

use std::io::{Read, Seek, SeekFrom, Write};

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};
use rts_engine::heap::poly::POLY_UNDEFINED;
use rts_engine::heap::shapes::handle_word_auto;

use super::callbacks::{err_object, str_word};
use super::codec::encode_bytes;
use super::stats;
use super::words::{byte_array, read, read_bytes};

unsafe extern "C" {
    fn __RTS_FN_GL_PROMISE_RESOLVE(value: u64) -> i64;
    fn __RTS_FN_GL_PROMISE_REJECT(reason: u64) -> i64;
}

fn resolve(value: u64) -> u64 {
    unsafe { __RTS_FN_GL_PROMISE_RESOLVE(value) as u64 }
}

fn reject(e: &std::io::Error, op: &str, path: &str) -> u64 {
    unsafe { __RTS_FN_GL_PROMISE_REJECT(err_object(e, op, path)) as u64 }
}

fn settle_void(r: std::io::Result<()>, op: &str) -> u64 {
    match r {
        Ok(()) => resolve(POLY_UNDEFINED),
        Err(e) => reject(&e, op, ""),
    }
}

fn ebadf() -> std::io::Error {
    std::io::Error::from_raw_os_error(9) // EBADF
}

/// The fd stored on a `FileHandle` instance, or `None` if it is closed/invalid.
fn fd_of(this: u64) -> Option<u64> {
    with_entry(this, |e| match e {
        Some(Entry::Map(m)) => m.get("fd").map(|&v| v as u64),
        _ => None,
    })
}

/// `fs.promises.open(path, flags)` → `Promise<FileHandle>`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_P_OPEN(p: *const u8, l: i64, fp: *const u8, fl: i64) -> u64 {
    let path = read(p, l);
    let flags = read(fp, fl);
    match super::fd::open_path(&path, &flags) {
        Ok(fd) => resolve(build_filehandle(fd)),
        Err(e) => reject(&e, "open", &path),
    }
}

/// Build the object-backed `FileHandle` word for an open `fd`.
pub(super) fn build_filehandle(fd: u64) -> u64 {
    let mut m: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
    m.insert(
        "__rts_class".to_string(),
        alloc_entry(Entry::String(b"FileHandle".to_vec())) as i64,
    );
    m.insert("fd".to_string(), fd as i64);
    handle_word_auto(alloc_entry(Entry::Map(Box::new(m))))
}

/// `engine.fs_open_handle(path, flags)` — the SYNC FileHandle opener for the `.ts`
/// `promises.open` wrapper (which then attaches the stream-method closures). All
/// words in/out: returns the object-backed `FileHandle` word, or throws a
/// Node-style error on failure (the wrapper does not catch — `open` rejects).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_FS_OPEN_HANDLE(path_word: u64, flags_word: u64) -> u64 {
    let path = super::words::word_string(path_word);
    let flags = super::words::word_string(flags_word);
    match super::fd::open_path(&path, &flags) {
        Ok(fd) => build_filehandle(fd),
        Err(e) => {
            super::words::throw_io(&e, "open", &path);
            POLY_UNDEFINED
        }
    }
}

/// `filehandle.close()` → `Promise<void>`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_FH_CLOSE(this: u64) -> u64 {
    if let Some(fd) = fd_of(this) {
        super::fd::close_fd(fd);
    }
    resolve(POLY_UNDEFINED)
}

/// Read the whole file behind the handle (from offset 0).
fn read_all(this: u64) -> std::io::Result<Vec<u8>> {
    let fd = fd_of(this).ok_or_else(ebadf)?;
    super::fd::with_file(fd, |f| {
        f.seek(SeekFrom::Start(0))?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        Ok(buf)
    })
    .unwrap_or_else(|| Err(ebadf()))
}

/// `filehandle.readFile()` → `Promise<Buffer>`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_FH_READ_FILE(this: u64) -> u64 {
    match read_all(this) {
        Ok(bytes) => resolve(handle_word_auto(byte_array(&bytes))),
        Err(e) => reject(&e, "read", ""),
    }
}

/// `filehandle.readFile(encoding)` → `Promise<string>`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_FH_READ_FILE_ENC(this: u64, ep: *const u8, el: i64) -> u64 {
    let enc = read(ep, el);
    match read_all(this) {
        Ok(bytes) => resolve(str_word(&encode_bytes(&bytes, &enc))),
        Err(e) => reject(&e, "read", ""),
    }
}

/// `filehandle.writeFile(data)` → `Promise<void>` (truncates then writes).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_FH_WRITE_FILE(this: u64, data: u64) -> u64 {
    let Some(fd) = fd_of(this) else { return reject(&ebadf(), "write", "") };
    let bytes = read_bytes(data);
    let r = super::fd::with_file(fd, |f| {
        f.seek(SeekFrom::Start(0))?;
        f.set_len(0)?;
        f.write_all(&bytes)
    })
    .unwrap_or_else(|| Err(ebadf()));
    settle_void(r, "write")
}

/// `filehandle.stat()` → `Promise<Stats>`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_FH_STAT(this: u64) -> u64 {
    let Some(fd) = fd_of(this) else { return reject(&ebadf(), "fstat", "") };
    match super::fd::with_file(fd, |f| f.metadata()) {
        Some(Ok(m)) => resolve(handle_word_auto(stats::build(&m))),
        Some(Err(e)) => reject(&e, "fstat", ""),
        None => reject(&ebadf(), "fstat", ""),
    }
}

/// `filehandle.truncate(len)` → `Promise<void>`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_FH_TRUNCATE(this: u64, len: i64) -> u64 {
    let Some(fd) = fd_of(this) else { return reject(&ebadf(), "ftruncate", "") };
    let r = super::fd::with_file(fd, |f| f.set_len(len.max(0) as u64)).unwrap_or_else(|| Err(ebadf()));
    settle_void(r, "ftruncate")
}

/// `filehandle.sync()` → `Promise<void>` (flush data + metadata).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_FH_SYNC(this: u64) -> u64 {
    let Some(fd) = fd_of(this) else { return reject(&ebadf(), "fsync", "") };
    let r = super::fd::with_file(fd, |f| f.sync_all()).unwrap_or_else(|| Err(ebadf()));
    settle_void(r, "fsync")
}

/// `filehandle.datasync()` → `Promise<void>` (flush data only).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_FH_DATASYNC(this: u64) -> u64 {
    let Some(fd) = fd_of(this) else { return reject(&ebadf(), "fdatasync", "") };
    let r = super::fd::with_file(fd, |f| f.sync_data()).unwrap_or_else(|| Err(ebadf()));
    settle_void(r, "fdatasync")
}

/// `filehandle.chmod(mode)` → `Promise<void>`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_FH_CHMOD(this: u64, mode: i64) -> u64 {
    let Some(fd) = fd_of(this) else { return reject(&ebadf(), "fchmod", "") };
    let r = super::fd::with_file(fd, |f| {
        let md = f.metadata()?;
        let mut perms = md.permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(mode as u32);
        }
        #[cfg(not(unix))]
        {
            perms.set_readonly(mode & 0o200 == 0);
        }
        f.set_permissions(perms)
    })
    .unwrap_or_else(|| Err(ebadf()));
    settle_void(r, "fchmod")
}
