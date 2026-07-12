//! node:fs — the `fs.promises` / `node:fs/promises` surface: the Promise-returning
//! forms of the core filesystem functions. The work is synchronous (the RTS event
//! loop is interim-synchronous, #207); each function runs the sync `std::fs` op
//! and returns an ALREADY-settled Promise (fulfilled with the result, or rejected
//! with a Node-style `{ message, code }` error) via the runtime Promise
//! primitives. `await fs.promises.readFile(p)` therefore resolves immediately.

use std::fs::OpenOptions;
use std::io::Write;

use rts_engine::heap::poly::POLY_UNDEFINED;
use rts_engine::heap::shapes::handle_word_auto;

use super::callbacks::{err_object, str_word};
use super::codec::encode_bytes;
use super::stats;
use super::words::{byte_array, read, read_bytes, string_array};

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

/// Settle a `void`-result op: a fulfilled `undefined` promise, or a rejection.
fn settle_void(r: std::io::Result<()>, op: &str, path: &str) -> u64 {
    match r {
        Ok(()) => resolve(POLY_UNDEFINED),
        Err(e) => reject(&e, op, path),
    }
}

/// `fs.promises.readFile(path)` → `Promise<Buffer>`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_P_READ_FILE(p: *const u8, l: i64) -> u64 {
    let path = read(p, l);
    match std::fs::read(&path) {
        Ok(bytes) => resolve(handle_word_auto(byte_array(&bytes))),
        Err(e) => reject(&e, "open", &path),
    }
}

/// `fs.promises.readFile(path, encoding)` → `Promise<string>`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_P_READ_FILE_ENC(p: *const u8, l: i64, ep: *const u8, el: i64) -> u64 {
    let path = read(p, l);
    let enc = read(ep, el);
    match std::fs::read(&path) {
        Ok(bytes) => resolve(str_word(&encode_bytes(&bytes, &enc))),
        Err(e) => reject(&e, "open", &path),
    }
}

/// `fs.promises.writeFile(path, data)` → `Promise<void>`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_P_WRITE_FILE(p: *const u8, l: i64, data: u64) -> u64 {
    let path = read(p, l);
    settle_void(std::fs::write(&path, read_bytes(data)), "open", &path)
}

/// `fs.promises.appendFile(path, data)` → `Promise<void>`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_P_APPEND_FILE(p: *const u8, l: i64, data: u64) -> u64 {
    let path = read(p, l);
    let r = (|| -> std::io::Result<()> {
        let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
        f.write_all(&read_bytes(data))
    })();
    settle_void(r, "open", &path)
}

/// `fs.promises.mkdir(path)` → `Promise<void>`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_P_MKDIR(p: *const u8, l: i64) -> u64 {
    let path = read(p, l);
    settle_void(std::fs::create_dir(&path), "mkdir", &path)
}

/// `fs.promises.unlink(path)` → `Promise<void>`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_P_UNLINK(p: *const u8, l: i64) -> u64 {
    let path = read(p, l);
    settle_void(std::fs::remove_file(&path), "unlink", &path)
}

/// `fs.promises.rmdir(path)` → `Promise<void>`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_P_RMDIR(p: *const u8, l: i64) -> u64 {
    let path = read(p, l);
    settle_void(std::fs::remove_dir(&path), "rmdir", &path)
}

/// `fs.promises.rm(path)` → `Promise<void>` (a file or empty directory).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_P_RM(p: *const u8, l: i64) -> u64 {
    let path = read(p, l);
    let r = std::fs::remove_file(&path).or_else(|_| std::fs::remove_dir(&path));
    settle_void(r, "unlink", &path)
}

/// `fs.promises.rename(oldPath, newPath)` → `Promise<void>`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_P_RENAME(op: *const u8, ol: i64, np: *const u8, nl: i64) -> u64 {
    let old = read(op, ol);
    let new = read(np, nl);
    settle_void(std::fs::rename(&old, &new), "rename", &old)
}

/// `fs.promises.copyFile(src, dest)` → `Promise<void>`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_P_COPY_FILE(sp: *const u8, sl: i64, dp: *const u8, dl: i64) -> u64 {
    let src = read(sp, sl);
    let dst = read(dp, dl);
    settle_void(std::fs::copy(&src, &dst).map(|_| ()), "copyfile", &src)
}

/// `fs.promises.access(path)` → `Promise<void>`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_P_ACCESS(p: *const u8, l: i64) -> u64 {
    let path = read(p, l);
    settle_void(std::fs::metadata(&path).map(|_| ()), "access", &path)
}

/// `fs.promises.truncate(path, len)` → `Promise<void>`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_P_TRUNCATE(p: *const u8, l: i64, len: i64) -> u64 {
    let path = read(p, l);
    let r = (|| -> std::io::Result<()> {
        OpenOptions::new().write(true).open(&path)?.set_len(len.max(0) as u64)
    })();
    settle_void(r, "ftruncate", &path)
}

/// `fs.promises.stat(path)` / `lstat(path)` → `Promise<Stats>`.
fn stat_p(path: String, follow: bool, op: &str) -> u64 {
    let md = if follow { std::fs::metadata(&path) } else { std::fs::symlink_metadata(&path) };
    match md {
        Ok(m) => resolve(handle_word_auto(stats::build(&m))),
        Err(e) => reject(&e, op, &path),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_P_STAT(p: *const u8, l: i64) -> u64 {
    stat_p(read(p, l), true, "stat")
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_P_LSTAT(p: *const u8, l: i64) -> u64 {
    stat_p(read(p, l), false, "lstat")
}

/// `fs.promises.readdir(path)` → `Promise<string[]>`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_P_READDIR(p: *const u8, l: i64) -> u64 {
    let path = read(p, l);
    match std::fs::read_dir(&path) {
        Ok(rd) => {
            let names: Vec<String> = rd
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            resolve(handle_word_auto(string_array(&names)))
        }
        Err(e) => reject(&e, "scandir", &path),
    }
}

/// `fs.promises.realpath(path)` → `Promise<string>`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_P_REALPATH(p: *const u8, l: i64) -> u64 {
    let path = read(p, l);
    match std::fs::canonicalize(&path) {
        Ok(rp) => resolve(str_word(&rp.to_string_lossy())),
        Err(e) => reject(&e, "realpath", &path),
    }
}

/// `fs.promises.readlink(path)` → `Promise<string>`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_P_READLINK(p: *const u8, l: i64) -> u64 {
    let path = read(p, l);
    match std::fs::read_link(&path) {
        Ok(target) => resolve(str_word(&target.to_string_lossy())),
        Err(e) => reject(&e, "readlink", &path),
    }
}
