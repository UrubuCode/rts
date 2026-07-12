//! node:fs — the callback (`err`-first) forms of the core filesystem functions.
//! The work is synchronous (the RTS event loop is interim-synchronous, #207);
//! the callback is invoked once, immediately, with Node's `callback(err, result)`
//! shape via the codegen callback bridge (`__rtsadp_fn_invoke`, the same one
//! dns/timers use). Real `std::fs` I/O — no stubs.

use std::fs::OpenOptions;
use std::io::Write;

use rts_engine::heap::poly::POLY_UNDEFINED;
use rts_engine::heap::shapes::{alloc_shaped_object, bool_word, handle_word_auto, null_word, string_word};

use super::codec::encode_bytes;
use super::stats;
use super::words::{err_code, read, read_bytes};

unsafe extern "C" {
    fn __rtsadp_fn_invoke(f: u64, a0: u64, a1: u64, a2: u64, a3: u64, this: u64) -> u64;
}

/// A boxed string PolyValue word for `s`.
pub(super) fn str_word(s: &str) -> u64 {
    string_word(s.as_bytes())
}

/// A Node-style error object `{ message, code }` (truthy → the `if (err)` path).
pub(super) fn err_object(e: &std::io::Error, op: &str, path: &str) -> u64 {
    let code = err_code(e);
    let msg = format!("{code}: {e}, {op} '{path}'");
    let h = alloc_shaped_object(
        &["message", "code"],
        &[string_word(msg.as_bytes()) as i64, string_word(code.as_bytes()) as i64],
    );
    handle_word_auto(h)
}

fn invoke(cb: u64, a0: u64, a1: u64) {
    unsafe { __rtsadp_fn_invoke(cb, a0, a1, POLY_UNDEFINED, POLY_UNDEFINED, POLY_UNDEFINED) };
}

/// Deliver a `void`-result op: `cb(null)` on success, `cb(err)` on failure.
fn settle_void(cb: u64, r: std::io::Result<()>, op: &str, path: &str) {
    match r {
        Ok(()) => invoke(cb, null_word(), POLY_UNDEFINED),
        Err(e) => invoke(cb, err_object(&e, op, path), POLY_UNDEFINED),
    }
}

/// `fs.readFile(path, callback)` → `callback(err, data)` with `data` a Buffer.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_CB_READ_FILE(p: *const u8, l: i64, cb: u64) {
    let path = read(p, l);
    match std::fs::read(&path) {
        Ok(bytes) => invoke(cb, null_word(), handle_word_auto(super::words::byte_array(&bytes))),
        Err(e) => invoke(cb, err_object(&e, "open", &path), POLY_UNDEFINED),
    }
}

/// `fs.readFile(path, encoding, callback)` → `callback(err, string)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_CB_READ_FILE_ENC(p: *const u8, l: i64, ep: *const u8, el: i64, cb: u64) {
    let path = read(p, l);
    let enc = read(ep, el);
    match std::fs::read(&path) {
        Ok(bytes) => invoke(cb, null_word(), str_word(&encode_bytes(&bytes, &enc))),
        Err(e) => invoke(cb, err_object(&e, "open", &path), POLY_UNDEFINED),
    }
}

/// `fs.writeFile(path, data, callback)` → `callback(err)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_CB_WRITE_FILE(p: *const u8, l: i64, data: u64, cb: u64) {
    let path = read(p, l);
    settle_void(cb, std::fs::write(&path, read_bytes(data)), "open", &path);
}

/// `fs.appendFile(path, data, callback)` → `callback(err)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_CB_APPEND_FILE(p: *const u8, l: i64, data: u64, cb: u64) {
    let path = read(p, l);
    let r = (|| -> std::io::Result<()> {
        let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
        f.write_all(&read_bytes(data))
    })();
    settle_void(cb, r, "open", &path);
}

/// `fs.mkdir(path, callback)` → `callback(err)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_CB_MKDIR(p: *const u8, l: i64, cb: u64) {
    let path = read(p, l);
    settle_void(cb, std::fs::create_dir(&path), "mkdir", &path);
}

/// `fs.unlink(path, callback)` → `callback(err)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_CB_UNLINK(p: *const u8, l: i64, cb: u64) {
    let path = read(p, l);
    settle_void(cb, std::fs::remove_file(&path), "unlink", &path);
}

/// `fs.rmdir(path, callback)` → `callback(err)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_CB_RMDIR(p: *const u8, l: i64, cb: u64) {
    let path = read(p, l);
    settle_void(cb, std::fs::remove_dir(&path), "rmdir", &path);
}

/// `fs.rm(path, callback)` → `callback(err)` (a file or empty directory).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_CB_RM(p: *const u8, l: i64, cb: u64) {
    let path = read(p, l);
    let r = std::fs::remove_file(&path).or_else(|_| std::fs::remove_dir(&path));
    settle_void(cb, r, "unlink", &path);
}

/// `fs.rename(oldPath, newPath, callback)` → `callback(err)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_CB_RENAME(op: *const u8, ol: i64, np: *const u8, nl: i64, cb: u64) {
    let old = read(op, ol);
    let new = read(np, nl);
    settle_void(cb, std::fs::rename(&old, &new), "rename", &old);
}

/// `fs.copyFile(src, dest, callback)` → `callback(err)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_CB_COPY_FILE(sp: *const u8, sl: i64, dp: *const u8, dl: i64, cb: u64) {
    let src = read(sp, sl);
    let dst = read(dp, dl);
    settle_void(cb, std::fs::copy(&src, &dst).map(|_| ()), "copyfile", &src);
}

/// `fs.access(path, callback)` → `callback(err)` (err when the path is absent).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_CB_ACCESS(p: *const u8, l: i64, cb: u64) {
    let path = read(p, l);
    settle_void(cb, std::fs::metadata(&path).map(|_| ()), "access", &path);
}

/// `fs.chmod(path, mode, callback)` → `callback(err)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_CB_CHMOD(p: *const u8, l: i64, mode: i64, cb: u64) {
    let path = read(p, l);
    let r = (|| -> std::io::Result<()> {
        let mut perms = std::fs::metadata(&path)?.permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(mode as u32);
        }
        #[cfg(not(unix))]
        {
            perms.set_readonly(mode & 0o200 == 0);
        }
        std::fs::set_permissions(&path, perms)
    })();
    settle_void(cb, r, "chmod", &path);
}

/// `fs.stat(path, callback)` / `fs.lstat(path, callback)` → `callback(err, Stats)`.
fn stat_cb(path: String, cb: u64, follow: bool, op: &str) {
    let md = if follow { std::fs::metadata(&path) } else { std::fs::symlink_metadata(&path) };
    match md {
        Ok(m) => invoke(cb, null_word(), handle_word_auto(stats::build(&m))),
        Err(e) => invoke(cb, err_object(&e, op, &path), POLY_UNDEFINED),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_CB_STAT(p: *const u8, l: i64, cb: u64) {
    stat_cb(read(p, l), cb, true, "stat");
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_CB_LSTAT(p: *const u8, l: i64, cb: u64) {
    stat_cb(read(p, l), cb, false, "lstat");
}

/// `fs.readdir(path, callback)` → `callback(err, files)` (string names).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_CB_READDIR(p: *const u8, l: i64, cb: u64) {
    let path = read(p, l);
    match std::fs::read_dir(&path) {
        Ok(rd) => {
            let names: Vec<String> = rd
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            invoke(cb, null_word(), handle_word_auto(super::words::string_array(&names)));
        }
        Err(e) => invoke(cb, err_object(&e, "scandir", &path), POLY_UNDEFINED),
    }
}

/// `fs.realpath(path, callback)` → `callback(err, resolvedPath)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_CB_REALPATH(p: *const u8, l: i64, cb: u64) {
    let path = read(p, l);
    match std::fs::canonicalize(&path) {
        Ok(rp) => invoke(cb, null_word(), str_word(&rp.to_string_lossy())),
        Err(e) => invoke(cb, err_object(&e, "realpath", &path), POLY_UNDEFINED),
    }
}

/// `fs.exists(path, callback)` → `callback(exists)` — the ONE fs callback without
/// an `err` argument (a single boolean; deprecated but present).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_CB_EXISTS(p: *const u8, l: i64, cb: u64) {
    let path = read(p, l);
    invoke(cb, bool_word(std::fs::metadata(&path).is_ok()), POLY_UNDEFINED);
}
