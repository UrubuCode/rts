//! node:fs — the callback (`err`-first) forms of the core filesystem functions.
//! The work is synchronous (the RTS event loop is interim-synchronous, #207);
//! the callback is invoked once, immediately, with Node's `callback(err, result)`
//! shape via the codegen callback bridge (`__rtsadp_fn_invoke`, the same one
//! dns/timers use). Real `std::fs` I/O — no stubs.
//!
//! Authored with `#[rtse::function]` (symbol/signature/ts/doc all derived from
//! the Rust declaration); the macro fixes `MemberFlags::NONE`, so `fs/mod.rs`
//! patches `THROWS` on at registration (matching the previous `func(...)` rows —
//! even though every one of these settles via a callback invocation, never an
//! actual `throw`, kept for parity with the prior flags).

use std::fs::OpenOptions;
use std::io::Write;

use rts_engine::abi::ty::{Handle, Poly};
use rts_engine::heap::poly::POLY_UNDEFINED;
use rts_engine::heap::shapes::{alloc_shaped_object, bool_word, handle_word_auto, null_word, string_word};

use super::codec::encode_bytes;
use super::stats;
use super::words::{err_code, read_bytes};

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
#[rtse::function(module = "node:fs", value = "readFile")]
fn read_file(path: &str, callback: Poly) {
    match std::fs::read(path) {
        Ok(bytes) => invoke(callback, null_word(), handle_word_auto(super::words::byte_array(&bytes))),
        Err(e) => invoke(callback, err_object(&e, "open", path), POLY_UNDEFINED),
    }
}

/// `fs.readFile(path, encoding, callback)` → `callback(err, string)`.
#[rtse::function(module = "node:fs", value = "readFile", overload = "enc")]
fn read_file_enc(path: &str, encoding: &str, callback: Poly) {
    match std::fs::read(path) {
        Ok(bytes) => invoke(callback, null_word(), str_word(&encode_bytes(&bytes, encoding))),
        Err(e) => invoke(callback, err_object(&e, "open", path), POLY_UNDEFINED),
    }
}

/// `fs.writeFile(path, data, callback)` → `callback(err)`.
#[rtse::function(module = "node:fs", value = "writeFile")]
fn write_file(path: &str, data: Handle, callback: Poly) {
    settle_void(callback, std::fs::write(path, read_bytes(data)), "open", path);
}

/// `fs.appendFile(path, data, callback)` → `callback(err)`.
#[rtse::function(module = "node:fs", value = "appendFile")]
fn append_file(path: &str, data: Handle, callback: Poly) {
    let r = (|| -> std::io::Result<()> {
        let mut f = OpenOptions::new().create(true).append(true).open(path)?;
        f.write_all(&read_bytes(data))
    })();
    settle_void(callback, r, "open", path);
}

/// `fs.mkdir(path, callback)` → `callback(err)`.
#[rtse::function(module = "node:fs", value = "mkdir")]
fn mkdir(path: &str, callback: Poly) {
    settle_void(callback, std::fs::create_dir(path), "mkdir", path);
}

/// `fs.unlink(path, callback)` → `callback(err)`.
#[rtse::function(module = "node:fs", value = "unlink")]
fn unlink(path: &str, callback: Poly) {
    settle_void(callback, std::fs::remove_file(path), "unlink", path);
}

/// `fs.rmdir(path, callback)` → `callback(err)`.
#[rtse::function(module = "node:fs", value = "rmdir")]
fn rmdir(path: &str, callback: Poly) {
    settle_void(callback, std::fs::remove_dir(path), "rmdir", path);
}

/// `fs.rm(path, callback)` → `callback(err)` (a file or empty directory).
#[rtse::function(module = "node:fs", value = "rm")]
fn rm(path: &str, callback: Poly) {
    let r = std::fs::remove_file(path).or_else(|_| std::fs::remove_dir(path));
    settle_void(callback, r, "unlink", path);
}

/// `fs.rename(oldPath, newPath, callback)` → `callback(err)`.
#[rtse::function(module = "node:fs", value = "rename")]
fn rename(old_path: &str, new_path: &str, callback: Poly) {
    settle_void(callback, std::fs::rename(old_path, new_path), "rename", old_path);
}

/// `fs.copyFile(src, dest, callback)` → `callback(err)`.
#[rtse::function(module = "node:fs", value = "copyFile")]
fn copy_file(src: &str, dest: &str, callback: Poly) {
    settle_void(callback, std::fs::copy(src, dest).map(|_| ()), "copyfile", src);
}

/// `fs.access(path, callback)` → `callback(err)` (err when the path is absent).
#[rtse::function(module = "node:fs", value = "access")]
fn access(path: &str, callback: Poly) {
    settle_void(callback, std::fs::metadata(path).map(|_| ()), "access", path);
}

/// `fs.chmod(path, mode, callback)` → `callback(err)`.
#[rtse::function(module = "node:fs", value = "chmod")]
fn chmod(path: &str, mode: i64, callback: Poly) {
    let r = (|| -> std::io::Result<()> {
        let mut perms = std::fs::metadata(path)?.permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(mode as u32);
        }
        #[cfg(not(unix))]
        {
            perms.set_readonly(mode & 0o200 == 0);
        }
        std::fs::set_permissions(path, perms)
    })();
    settle_void(callback, r, "chmod", path);
}

/// `fs.stat(path, callback)` / `fs.lstat(path, callback)` → `callback(err, Stats)`.
fn stat_cb(path: &str, cb: u64, follow: bool, op: &str) {
    let md = if follow { std::fs::metadata(path) } else { std::fs::symlink_metadata(path) };
    match md {
        Ok(m) => invoke(cb, null_word(), handle_word_auto(stats::build(&m))),
        Err(e) => invoke(cb, err_object(&e, op, path), POLY_UNDEFINED),
    }
}

#[rtse::function(module = "node:fs", value = "stat")]
fn stat(path: &str, callback: Poly) {
    stat_cb(path, callback, true, "stat");
}

#[rtse::function(module = "node:fs", value = "lstat")]
fn lstat(path: &str, callback: Poly) {
    stat_cb(path, callback, false, "lstat");
}

/// `fs.readdir(path, callback)` → `callback(err, files)` (string names).
#[rtse::function(module = "node:fs", value = "readdir")]
fn readdir(path: &str, callback: Poly) {
    match std::fs::read_dir(path) {
        Ok(rd) => {
            let names: Vec<String> = rd
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            invoke(callback, null_word(), handle_word_auto(super::words::string_array(&names)));
        }
        Err(e) => invoke(callback, err_object(&e, "scandir", path), POLY_UNDEFINED),
    }
}

/// `fs.realpath(path, callback)` → `callback(err, resolvedPath)`.
#[rtse::function(module = "node:fs", value = "realpath")]
fn realpath(path: &str, callback: Poly) {
    match std::fs::canonicalize(path) {
        Ok(rp) => invoke(callback, null_word(), str_word(&rp.to_string_lossy())),
        Err(e) => invoke(callback, err_object(&e, "realpath", path), POLY_UNDEFINED),
    }
}

/// `fs.exists(path, callback)` → `callback(exists)` — the ONE fs callback without
/// an `err` argument (a single boolean; deprecated but present).
#[rtse::function(module = "node:fs", value = "exists")]
fn exists(path: &str, callback: Poly) {
    invoke(callback, bool_word(std::fs::metadata(path).is_ok()), POLY_UNDEFINED);
}
