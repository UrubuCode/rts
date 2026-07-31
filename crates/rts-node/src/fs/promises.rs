//! node:fs — the `fs.promises` / `node:fs/promises` surface: the Promise-returning
//! forms of the core filesystem functions. The work is synchronous (the RTS event
//! loop is interim-synchronous, #207); each function runs the sync `std::fs` op
//! and returns an ALREADY-settled Promise (fulfilled with the result, or rejected
//! with a Node-style `{ message, code }` error) via the runtime Promise
//! primitives. `await fs.promises.readFile(p)` therefore resolves immediately.
//!
//! Authored with `#[rtse::function]` (symbol/signature/ts/doc all derived from
//! the Rust declaration); the macro fixes `MemberFlags::NONE`, so `fs/mod.rs`
//! patches `THROWS` on at registration (matching the previous `func(...)` rows,
//! even though every failure here settles as a REJECTED Promise rather than an
//! actual `throw` — kept for parity with the prior flags).

use std::fs::OpenOptions;
use std::io::Write;

use rts_engine::abi::ty::Handle;
use rts_engine::heap::poly::POLY_UNDEFINED;
use rts_engine::heap::shapes::handle_word_auto;

use super::callbacks::{err_object, str_word};
use super::codec::encode_bytes;
use super::stats;
use super::words::{byte_array, read_bytes, string_array};

use rts_engine::externs::{__RTS_FN_GL_PROMISE_REJECT, __RTS_FN_GL_PROMISE_RESOLVE};

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
#[rtse::function(module = "node:fs/promises", value = "readFile")]
fn read_file(path: &str) -> Handle {
    match std::fs::read(path) {
        Ok(bytes) => resolve(handle_word_auto(byte_array(&bytes))),
        Err(e) => reject(&e, "open", path),
    }
}

/// `fs.promises.readFile(path, encoding)` → `Promise<string>`.
#[rtse::function(module = "node:fs/promises", value = "readFile", overload = "enc")]
fn read_file_enc(path: &str, encoding: &str) -> Handle {
    match std::fs::read(path) {
        Ok(bytes) => resolve(str_word(&encode_bytes(&bytes, encoding))),
        Err(e) => reject(&e, "open", path),
    }
}

/// `fs.promises.writeFile(path, data)` → `Promise<void>`.
#[rtse::function(module = "node:fs/promises", value = "writeFile")]
fn write_file(path: &str, data: Handle) -> Handle {
    settle_void(std::fs::write(path, read_bytes(data)), "open", path)
}

/// `fs.promises.appendFile(path, data)` → `Promise<void>`.
#[rtse::function(module = "node:fs/promises", value = "appendFile")]
fn append_file(path: &str, data: Handle) -> Handle {
    let r = (|| -> std::io::Result<()> {
        let mut f = OpenOptions::new().create(true).append(true).open(path)?;
        f.write_all(&read_bytes(data))
    })();
    settle_void(r, "open", path)
}

/// `fs.promises.mkdir(path)` → `Promise<void>`.
#[rtse::function(module = "node:fs/promises", value = "mkdir")]
fn mkdir(path: &str) -> Handle {
    settle_void(std::fs::create_dir(path), "mkdir", path)
}

/// `fs.promises.unlink(path)` → `Promise<void>`.
#[rtse::function(module = "node:fs/promises", value = "unlink")]
fn unlink(path: &str) -> Handle {
    settle_void(std::fs::remove_file(path), "unlink", path)
}

/// `fs.promises.rmdir(path)` → `Promise<void>`.
#[rtse::function(module = "node:fs/promises", value = "rmdir")]
fn rmdir(path: &str) -> Handle {
    settle_void(std::fs::remove_dir(path), "rmdir", path)
}

/// `fs.promises.rm(path)` → `Promise<void>` (a file or empty directory).
#[rtse::function(module = "node:fs/promises", value = "rm")]
fn rm(path: &str) -> Handle {
    let r = std::fs::remove_file(path).or_else(|_| std::fs::remove_dir(path));
    settle_void(r, "unlink", path)
}

/// `fs.promises.rename(oldPath, newPath)` → `Promise<void>`.
#[rtse::function(module = "node:fs/promises", value = "rename")]
fn rename(old_path: &str, new_path: &str) -> Handle {
    settle_void(std::fs::rename(old_path, new_path), "rename", old_path)
}

/// `fs.promises.copyFile(src, dest)` → `Promise<void>`.
#[rtse::function(module = "node:fs/promises", value = "copyFile")]
fn copy_file(src: &str, dest: &str) -> Handle {
    settle_void(std::fs::copy(src, dest).map(|_| ()), "copyfile", src)
}

/// `fs.promises.access(path)` → `Promise<void>`.
#[rtse::function(module = "node:fs/promises", value = "access")]
fn access(path: &str) -> Handle {
    settle_void(std::fs::metadata(path).map(|_| ()), "access", path)
}

/// `fs.promises.truncate(path, len)` → `Promise<void>`.
#[rtse::function(module = "node:fs/promises", value = "truncate")]
fn truncate(path: &str, len: i64) -> Handle {
    let r = (|| -> std::io::Result<()> { OpenOptions::new().write(true).open(path)?.set_len(len.max(0) as u64) })();
    settle_void(r, "ftruncate", path)
}

/// `fs.promises.stat(path)` / `lstat(path)` → `Promise<Stats>`.
fn stat_p(path: &str, follow: bool, op: &str) -> u64 {
    let md = if follow { std::fs::metadata(path) } else { std::fs::symlink_metadata(path) };
    match md {
        Ok(m) => resolve(handle_word_auto(stats::build(&m))),
        Err(e) => reject(&e, op, path),
    }
}

#[rtse::function(module = "node:fs/promises", value = "stat")]
fn stat(path: &str) -> Handle {
    stat_p(path, true, "stat")
}

#[rtse::function(module = "node:fs/promises", value = "lstat")]
fn lstat(path: &str) -> Handle {
    stat_p(path, false, "lstat")
}

/// `fs.promises.readdir(path)` → `Promise<string[]>`.
#[rtse::function(module = "node:fs/promises", value = "readdir")]
fn readdir(path: &str) -> Handle {
    match std::fs::read_dir(path) {
        Ok(rd) => {
            let names: Vec<String> = rd
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            resolve(handle_word_auto(string_array(&names)))
        }
        Err(e) => reject(&e, "scandir", path),
    }
}

/// `fs.promises.realpath(path)` → `Promise<string>`.
#[rtse::function(module = "node:fs/promises", value = "realpath")]
fn realpath(path: &str) -> Handle {
    match std::fs::canonicalize(path) {
        Ok(rp) => resolve(str_word(&rp.to_string_lossy())),
        Err(e) => reject(&e, "realpath", path),
    }
}

/// `fs.promises.readlink(path)` → `Promise<string>`.
#[rtse::function(module = "node:fs/promises", value = "readlink")]
fn readlink(path: &str) -> Handle {
    match std::fs::read_link(path) {
        Ok(target) => resolve(str_word(&target.to_string_lossy())),
        Err(e) => reject(&e, "readlink", path),
    }
}
