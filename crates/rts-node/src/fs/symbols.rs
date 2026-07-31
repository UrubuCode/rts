//! node:fs — the synchronous `extern "C"` entry points, each a thin wrapper over
//! `std::fs` that throws a Node-style error on failure. No fabricated results.
//!
//! The free functions here are authored with `#[rtse::function]`
//! (symbol/signature/ts/doc all derived from the Rust declaration); the macro
//! fixes `MemberFlags::NONE`, so `fs/mod.rs` patches `THROWS` on at registration
//! (`throws(...)`, matching `rts-shared/src/serde_ns`). `Stats` itself is now a
//! `#[rtse::class]` (see `stats.rs`) — its instance methods/getters no longer
//! live here.

use super::codec::{decode_bytes, encode_bytes};
use super::stats;
use super::words::{byte_array, opt_bool, read_bytes, string_array, throw_io};
use rts_engine::abi::ty::Handle;

/// `fs.constants` — the libuv access-mode + copyfile flags. Field-accessible
/// object (`fs.constants.F_OK`), real values (identical across platforms).
///
/// NOT converted: `MemberKind::Constant`, a kind `#[rtse::function]` cannot
/// express (it always emits `MemberKind::Function`).
#[rtse::abi("__RTS_FN_NODE_FS_CONSTANTS")]
pub fn __RTS_FN_NODE_FS_CONSTANTS() -> u64 {
    let num = |v: f64| v.to_bits() as i64;
    rts_engine::heap::shapes::alloc_shaped_object(
        &["F_OK", "R_OK", "W_OK", "X_OK", "COPYFILE_EXCL", "COPYFILE_FICLONE", "COPYFILE_FICLONE_FORCE"],
        &[num(0.0), num(4.0), num(2.0), num(1.0), num(1.0), num(2.0), num(4.0)],
    )
}

/// `fs.readFileSync(path)` → Buffer (Uint8Array-shaped).
#[rtse::function(module = "node:fs", value = "readFileSync")]
fn read_file_sync(path: &str) -> Handle {
    match std::fs::read(path) {
        Ok(bytes) => byte_array(&bytes),
        Err(e) => {
            throw_io(&e, "open", path);
            byte_array(&[])
        }
    }
}

/// `fs.readFileSync(path, encoding)` → string in the requested encoding
/// (utf8/hex/base64/base64url/latin1/ascii).
#[rtse::function(module = "node:fs", value = "readFileSync", overload = "enc")]
fn read_file_sync_enc(path: &str, encoding: &str) -> String {
    match std::fs::read(path) {
        Ok(bytes) => encode_bytes(&bytes, encoding),
        Err(e) => {
            throw_io(&e, "open", path);
            "".to_string()
        }
    }
}

/// `fs.writeFileSync(path, data)`.
#[rtse::function(module = "node:fs", value = "writeFileSync")]
fn write_file_sync(path: &str, data: Handle) {
    if let Err(e) = std::fs::write(path, read_bytes(data)) {
        throw_io(&e, "open", path);
    }
}

/// `fs.writeFileSync(path, data, encoding)` — the string data is decoded per the
/// encoding (utf8/hex/base64/latin1) before writing.
#[rtse::function(module = "node:fs", value = "writeFileSync", overload = "enc")]
fn write_file_sync_enc(path: &str, data: &str, encoding: &str) {
    let bytes = decode_bytes(data, encoding);
    if let Err(e) = std::fs::write(path, bytes) {
        throw_io(&e, "open", path);
    }
}

/// `fs.appendFileSync(path, data)`.
#[rtse::function(module = "node:fs", value = "appendFileSync")]
fn append_file_sync(path: &str, data: Handle) {
    use std::io::Write;
    let r = std::fs::OpenOptions::new().create(true).append(true).open(path).and_then(|mut f| f.write_all(&read_bytes(data)));
    if let Err(e) = r {
        throw_io(&e, "open", path);
    }
}

/// `fs.appendFileSync(path, data, encoding)` — decode then append.
#[rtse::function(module = "node:fs", value = "appendFileSync", overload = "enc")]
fn append_file_sync_enc(path: &str, data: &str, encoding: &str) {
    use std::io::Write;
    let bytes = decode_bytes(data, encoding);
    let r = std::fs::OpenOptions::new().create(true).append(true).open(path).and_then(|mut f| f.write_all(&bytes));
    if let Err(e) = r {
        throw_io(&e, "open", path);
    }
}

/// `fs.existsSync(path)`.
#[rtse::function(module = "node:fs", value = "existsSync")]
fn exists_sync(path: &str) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

/// `fs.accessSync(path)` — throws if the path does not exist (F_OK).
#[rtse::function(module = "node:fs", value = "accessSync")]
fn access_sync(path: &str) {
    if let Err(e) = std::fs::symlink_metadata(path) {
        throw_io(&e, "access", path);
    }
}

/// `fs.mkdirSync(path)` (non-recursive).
#[rtse::function(module = "node:fs", value = "mkdirSync")]
fn mkdir_sync(path: &str) {
    if let Err(e) = std::fs::create_dir(path) {
        throw_io(&e, "mkdir", path);
    }
}

/// `fs.mkdirSync(path, options)` — creates missing parents when
/// `options.recursive` is truthy, else a single directory.
#[rtse::function(module = "node:fs", value = "mkdirSync", overload = "opts")]
fn mkdir_sync_opts(path: &str, options: Handle) {
    let r = if opt_bool(options, "recursive") {
        std::fs::create_dir_all(path)
    } else {
        std::fs::create_dir(path)
    };
    if let Err(e) = r {
        throw_io(&e, "mkdir", path);
    }
}

/// `fs.rmdirSync(path)` (empty directory).
#[rtse::function(module = "node:fs", value = "rmdirSync")]
fn rmdir_sync(path: &str) {
    if let Err(e) = std::fs::remove_dir(path) {
        throw_io(&e, "rmdir", path);
    }
}

/// `fs.rmSync(path)` — remove a file or an empty directory (non-recursive).
#[rtse::function(module = "node:fs", value = "rmSync")]
fn rm_sync(path: &str) {
    rm_impl(path, false);
}

/// `fs.rmSync(path, options)` — recursive tree removal when `options.recursive`.
#[rtse::function(module = "node:fs", value = "rmSync", overload = "opts")]
fn rm_sync_opts(path: &str, options: Handle) {
    rm_impl(path, opt_bool(options, "recursive"));
}

fn rm_impl(path: &str, recursive: bool) {
    let r = match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => {
            if recursive { std::fs::remove_dir_all(path) } else { std::fs::remove_dir(path) }
        }
        Ok(_) => std::fs::remove_file(path),
        Err(e) => Err(e),
    };
    if let Err(e) = r {
        throw_io(&e, "rm", path);
    }
}

/// `fs.unlinkSync(path)`.
#[rtse::function(module = "node:fs", value = "unlinkSync")]
fn unlink_sync(path: &str) {
    if let Err(e) = std::fs::remove_file(path) {
        throw_io(&e, "unlink", path);
    }
}

/// `fs.renameSync(oldPath, newPath)`.
#[rtse::function(module = "node:fs", value = "renameSync")]
fn rename_sync(old_path: &str, new_path: &str) {
    if let Err(e) = std::fs::rename(old_path, new_path) {
        throw_io(&e, "rename", old_path);
    }
}

/// `fs.cpSync(src, dest)` — copy a single file (a directory needs the recursive
/// options form).
#[rtse::function(module = "node:fs", value = "cpSync")]
fn cp_sync(src: &str, dest: &str) {
    if let Err(e) = cp_impl(std::path::Path::new(src), std::path::Path::new(dest), false) {
        throw_io(&e, "cp", src);
    }
}

/// `fs.cpSync(src, dest, options)` — recursive tree copy when `options.recursive`.
#[rtse::function(module = "node:fs", value = "cpSync", overload = "opts")]
fn cp_sync_opts(src: &str, dest: &str, options: Handle) {
    let recursive = opt_bool(options, "recursive");
    if let Err(e) = cp_impl(std::path::Path::new(src), std::path::Path::new(dest), recursive) {
        throw_io(&e, "cp", src);
    }
}

fn cp_impl(src: &std::path::Path, dest: &std::path::Path, recursive: bool) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(src)?;
    if meta.is_dir() {
        if !recursive {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "cannot copy a directory without recursive:true"));
        }
        std::fs::create_dir_all(dest)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            cp_impl(&entry.path(), &dest.join(entry.file_name()), true)?;
        }
        Ok(())
    } else {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dest).map(|_| ())
    }
}

/// `fs.copyFileSync(src, dest)`.
#[rtse::function(module = "node:fs", value = "copyFileSync")]
fn copy_file_sync(src: &str, dest: &str) {
    if let Err(e) = std::fs::copy(src, dest) {
        throw_io(&e, "copyfile", src);
    }
}

/// `fs.utimesSync(path, atime, mtime)` — set the access/modify times (seconds
/// since the Unix epoch; fractional allowed).
#[rtse::function(module = "node:fs", value = "utimesSync")]
fn utimes_sync(path: &str, atime: f64, mtime: f64) {
    let to_time = |secs: f64| -> std::time::SystemTime {
        if secs >= 0.0 {
            std::time::UNIX_EPOCH + std::time::Duration::from_secs_f64(secs)
        } else {
            std::time::UNIX_EPOCH - std::time::Duration::from_secs_f64(-secs)
        }
    };
    let r = (|| -> std::io::Result<()> {
        let times = std::fs::FileTimes::new().set_accessed(to_time(atime)).set_modified(to_time(mtime));
        std::fs::OpenOptions::new().write(true).open(path)?.set_times(times)
    })();
    if let Err(e) = r {
        throw_io(&e, "utime", path);
    }
}

/// `fs.chmodSync(path, mode)`.
#[rtse::function(module = "node:fs", value = "chmodSync")]
fn chmod_sync(path: &str, mode: i64) {
    let r = (|| -> std::io::Result<()> {
        let mut perms = std::fs::metadata(path)?.permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(mode as u32);
        }
        #[cfg(not(unix))]
        {
            // Windows has no POSIX mode: map the owner-write bit to read-only.
            perms.set_readonly(mode & 0o200 == 0);
        }
        std::fs::set_permissions(path, perms)
    })();
    if let Err(e) = r {
        throw_io(&e, "chmod", path);
    }
}

/// `fs.linkSync(existingPath, newPath)` — create a hard link.
#[rtse::function(module = "node:fs", value = "linkSync")]
fn link_sync(existing_path: &str, new_path: &str) {
    if let Err(e) = std::fs::hard_link(existing_path, new_path) {
        throw_io(&e, "link", existing_path);
    }
}

/// `fs.truncateSync(path, len)`.
#[rtse::function(module = "node:fs", value = "truncateSync")]
fn truncate_sync(path: &str, len: i64) {
    let r = std::fs::OpenOptions::new().write(true).open(path).and_then(|f| f.set_len(len.max(0) as u64));
    if let Err(e) = r {
        throw_io(&e, "open", path);
    }
}

/// `fs.readdirSync(path)` → string[]. Paired with `dirent::readdir_sync_opts`
/// (`overload = "opts"` there) under the same JS name `readdirSync`.
#[rtse::function(module = "node:fs", value = "readdirSync")]
fn readdir_sync(path: &str) -> Handle {
    match std::fs::read_dir(path) {
        Ok(rd) => {
            let names: Vec<String> = rd
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            string_array(&names)
        }
        Err(e) => {
            throw_io(&e, "scandir", path);
            string_array(&[])
        }
    }
}

/// `fs.mkdtempSync(prefix)` → the created unique temp directory path. Node
/// appends 6 random `[A-Za-z0-9]` characters to `prefix`.
#[rtse::function(module = "node:fs", value = "mkdtempSync")]
fn mkdtemp_sync(prefix: &str) -> String {
    const ALPHABET: &[u8; 62] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    // Retry on the astronomically-unlikely collision; surface any other error.
    for _ in 0..64 {
        let mut rand = [0u8; 6];
        if getrandom::getrandom(&mut rand).is_err() {
            break;
        }
        let suffix: String = rand.iter().map(|&b| ALPHABET[(b % 62) as usize] as char).collect();
        let path = format!("{prefix}{suffix}");
        match std::fs::create_dir(&path) {
            Ok(()) => return path,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                throw_io(&e, "mkdtemp", &path);
                return String::new();
            }
        }
    }
    "".to_string()
}

/// `fs.readlinkSync(path)` → the symlink's target path.
#[rtse::function(module = "node:fs", value = "readlinkSync")]
fn readlink_sync(path: &str) -> String {
    match std::fs::read_link(path) {
        Ok(target) => target.to_string_lossy().into_owned(),
        Err(e) => {
            throw_io(&e, "readlink", path);
            "".to_string()
        }
    }
}

/// `fs.realpathSync(path)` → string.
#[rtse::function(module = "node:fs", value = "realpathSync")]
fn realpath_sync(path: &str) -> String {
    match std::fs::canonicalize(path) {
        Ok(pb) => pb.to_string_lossy().into_owned(),
        Err(e) => {
            throw_io(&e, "realpath", path);
            "".to_string()
        }
    }
}

/// `fs.statSync(path)` → Stats (follows symlinks).
#[rtse::function(module = "node:fs", value = "statSync", ret_ts = "Stats")]
fn stat_sync(path: &str) -> Handle {
    match std::fs::metadata(path) {
        Ok(m) => stats::build(&m),
        Err(e) => {
            throw_io(&e, "stat", path);
            0
        }
    }
}

/// `fs.lstatSync(path)` → Stats (does not follow symlinks).
#[rtse::function(module = "node:fs", value = "lstatSync", ret_ts = "Stats")]
fn lstat_sync(path: &str) -> Handle {
    match std::fs::symlink_metadata(path) {
        Ok(m) => stats::build(&m),
        Err(e) => {
            throw_io(&e, "lstat", path);
            0
        }
    }
}

// `Stats` instance methods/getters are now a `#[rtse::class]` (see `stats.rs`);
// nothing to hand-write here anymore.
