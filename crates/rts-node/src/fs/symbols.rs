//! node:fs — the synchronous `extern "C"` entry points, each a thin wrapper over
//! `std::fs` that throws a Node-style error on failure. No fabricated results.

use super::stats;
use super::words::{byte_array, intern, opt_bool, read, read_bytes, string_array, throw_io};

/// `fs.constants` — the libuv access-mode + copyfile flags. Field-accessible
/// object (`fs.constants.F_OK`), real values (identical across platforms).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_CONSTANTS() -> u64 {
    let num = |v: f64| v.to_bits() as i64;
    rts_engine::heap::shapes::alloc_shaped_object(
        &["F_OK", "R_OK", "W_OK", "X_OK", "COPYFILE_EXCL", "COPYFILE_FICLONE", "COPYFILE_FICLONE_FORCE"],
        &[num(0.0), num(4.0), num(2.0), num(1.0), num(1.0), num(2.0), num(4.0)],
    )
}

/// `fs.readFileSync(path)` → Buffer (Uint8Array-shaped).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_READ_FILE(p: *const u8, l: i64) -> u64 {
    let path = read(p, l);
    match std::fs::read(&path) {
        Ok(bytes) => byte_array(&bytes),
        Err(e) => {
            throw_io(&e, "open", &path);
            byte_array(&[])
        }
    }
}

/// `fs.readFileSync(path, encoding)` → string in the requested encoding
/// (utf8/hex/base64/base64url/latin1/ascii).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_READ_FILE_ENC(p: *const u8, l: i64, ep: *const u8, el: i64) -> u64 {
    let path = read(p, l);
    let enc = read(ep, el);
    match std::fs::read(&path) {
        Ok(bytes) => intern(&encode_bytes(&bytes, &enc)),
        Err(e) => {
            throw_io(&e, "open", &path);
            intern("")
        }
    }
}

/// Encode file bytes per a Node encoding name (default utf8).
fn encode_bytes(bytes: &[u8], enc: &str) -> String {
    match enc.to_lowercase().as_str() {
        "hex" => bytes.iter().map(|b| format!("{b:02x}")).collect(),
        "base64" => b64_encode(bytes, false),
        "base64url" => b64_encode(bytes, true),
        "latin1" | "binary" | "ascii" => bytes.iter().map(|&b| b as char).collect(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn b64_encode(data: &[u8], url: bool) -> String {
    const STD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    const URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let tbl = if url { URL } else { STD };
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(tbl[(n >> 18 & 63) as usize] as char);
        out.push(tbl[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 { tbl[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { tbl[(n & 63) as usize] as char } else { '=' });
    }
    if url { out.trim_end_matches('=').to_string() } else { out }
}

/// `fs.writeFileSync(path, data)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_WRITE_FILE(p: *const u8, l: i64, data: u64) {
    let path = read(p, l);
    if let Err(e) = std::fs::write(&path, read_bytes(data)) {
        throw_io(&e, "open", &path);
    }
}

/// `fs.writeFileSync(path, data, encoding)` — the string data is decoded per the
/// encoding (utf8/hex/base64/latin1) before writing.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_WRITE_FILE_ENC(p: *const u8, l: i64, dp: *const u8, dl: i64, ep: *const u8, el: i64) {
    let path = read(p, l);
    let bytes = decode_bytes(&read(dp, dl), &read(ep, el));
    if let Err(e) = std::fs::write(&path, bytes) {
        throw_io(&e, "open", &path);
    }
}

/// Decode a string to bytes per a Node encoding name (default utf8).
fn decode_bytes(s: &str, enc: &str) -> Vec<u8> {
    match enc.to_lowercase().as_str() {
        "hex" => (0..s.len() / 2)
            .filter_map(|i| u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok())
            .collect(),
        "base64" | "base64url" => b64_decode(s),
        "latin1" | "binary" | "ascii" => s.chars().map(|c| c as u8).collect(),
        _ => s.as_bytes().to_vec(),
    }
}

fn b64_decode(s: &str) -> Vec<u8> {
    let val = |c: u8| -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' | b'-' => Some(62),
            b'/' | b'_' => Some(63),
            _ => None,
        }
    };
    let clean: Vec<u8> = s.bytes().filter(|&c| val(c).is_some()).collect();
    let mut out = Vec::new();
    for chunk in clean.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= val(c).unwrap_or(0) << (18 - 6 * i);
        }
        out.push((n >> 16 & 0xff) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8 & 0xff) as u8);
        }
        if chunk.len() > 3 {
            out.push((n & 0xff) as u8);
        }
    }
    out
}

/// `fs.appendFileSync(path, data)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_APPEND_FILE(p: *const u8, l: i64, data: u64) {
    use std::io::Write;
    let path = read(p, l);
    let r = std::fs::OpenOptions::new().create(true).append(true).open(&path).and_then(|mut f| f.write_all(&read_bytes(data)));
    if let Err(e) = r {
        throw_io(&e, "open", &path);
    }
}

/// `fs.appendFileSync(path, data, encoding)` — decode then append.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_APPEND_FILE_ENC(p: *const u8, l: i64, dp: *const u8, dl: i64, ep: *const u8, el: i64) {
    use std::io::Write;
    let path = read(p, l);
    let bytes = decode_bytes(&read(dp, dl), &read(ep, el));
    let r = std::fs::OpenOptions::new().create(true).append(true).open(&path).and_then(|mut f| f.write_all(&bytes));
    if let Err(e) = r {
        throw_io(&e, "open", &path);
    }
}

/// `fs.existsSync(path)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_EXISTS(p: *const u8, l: i64) -> i64 {
    std::fs::symlink_metadata(read(p, l)).is_ok() as i64
}

/// `fs.accessSync(path)` — throws if the path does not exist (F_OK).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_ACCESS(p: *const u8, l: i64) {
    let path = read(p, l);
    if let Err(e) = std::fs::symlink_metadata(&path) {
        throw_io(&e, "access", &path);
    }
}

/// `fs.mkdirSync(path)` (non-recursive).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_MKDIR(p: *const u8, l: i64) {
    let path = read(p, l);
    if let Err(e) = std::fs::create_dir(&path) {
        throw_io(&e, "mkdir", &path);
    }
}

/// `fs.mkdirSync(path, options)` — creates missing parents when
/// `options.recursive` is truthy, else a single directory.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_MKDIR_OPTS(p: *const u8, l: i64, options: u64) {
    let path = read(p, l);
    let r = if opt_bool(options, "recursive") {
        std::fs::create_dir_all(&path)
    } else {
        std::fs::create_dir(&path)
    };
    if let Err(e) = r {
        throw_io(&e, "mkdir", &path);
    }
}

/// `fs.rmdirSync(path)` (empty directory).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_RMDIR(p: *const u8, l: i64) {
    let path = read(p, l);
    if let Err(e) = std::fs::remove_dir(&path) {
        throw_io(&e, "rmdir", &path);
    }
}

/// `fs.rmSync(path)` — remove a file or an empty directory (non-recursive).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_RM(p: *const u8, l: i64) {
    rm_impl(&read(p, l), false);
}

/// `fs.rmSync(path, options)` — recursive tree removal when `options.recursive`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_RM_OPTS(p: *const u8, l: i64, options: u64) {
    rm_impl(&read(p, l), opt_bool(options, "recursive"));
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_UNLINK(p: *const u8, l: i64) {
    let path = read(p, l);
    if let Err(e) = std::fs::remove_file(&path) {
        throw_io(&e, "unlink", &path);
    }
}

/// `fs.renameSync(oldPath, newPath)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_RENAME(op: *const u8, ol: i64, np: *const u8, nl: i64) {
    let (from, to) = (read(op, ol), read(np, nl));
    if let Err(e) = std::fs::rename(&from, &to) {
        throw_io(&e, "rename", &from);
    }
}

/// `fs.cpSync(src, dest)` — copy a single file (a directory needs the recursive
/// options form).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_CP(sp: *const u8, sl: i64, dp: *const u8, dl: i64) {
    let (src, dest) = (read(sp, sl), read(dp, dl));
    if let Err(e) = cp_impl(std::path::Path::new(&src), std::path::Path::new(&dest), false) {
        throw_io(&e, "cp", &src);
    }
}

/// `fs.cpSync(src, dest, options)` — recursive tree copy when `options.recursive`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_CP_OPTS(sp: *const u8, sl: i64, dp: *const u8, dl: i64, options: u64) {
    let (src, dest) = (read(sp, sl), read(dp, dl));
    let recursive = opt_bool(options, "recursive");
    if let Err(e) = cp_impl(std::path::Path::new(&src), std::path::Path::new(&dest), recursive) {
        throw_io(&e, "cp", &src);
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_COPY_FILE(sp: *const u8, sl: i64, dp: *const u8, dl: i64) {
    let (src, dest) = (read(sp, sl), read(dp, dl));
    if let Err(e) = std::fs::copy(&src, &dest) {
        throw_io(&e, "copyfile", &src);
    }
}

/// `fs.utimesSync(path, atime, mtime)` — set the access/modify times (seconds
/// since the Unix epoch; fractional allowed).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_UTIMES(p: *const u8, l: i64, atime: f64, mtime: f64) {
    let path = read(p, l);
    let to_time = |secs: f64| -> std::time::SystemTime {
        if secs >= 0.0 {
            std::time::UNIX_EPOCH + std::time::Duration::from_secs_f64(secs)
        } else {
            std::time::UNIX_EPOCH - std::time::Duration::from_secs_f64(-secs)
        }
    };
    let r = (|| -> std::io::Result<()> {
        let times = std::fs::FileTimes::new().set_accessed(to_time(atime)).set_modified(to_time(mtime));
        std::fs::OpenOptions::new().write(true).open(&path)?.set_times(times)
    })();
    if let Err(e) = r {
        throw_io(&e, "utime", &path);
    }
}

/// `fs.chmodSync(path, mode)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_CHMOD(p: *const u8, l: i64, mode: i64) {
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
            // Windows has no POSIX mode: map the owner-write bit to read-only.
            perms.set_readonly(mode & 0o200 == 0);
        }
        std::fs::set_permissions(&path, perms)
    })();
    if let Err(e) = r {
        throw_io(&e, "chmod", &path);
    }
}

/// `fs.linkSync(existingPath, newPath)` — create a hard link.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_LINK(ep: *const u8, el: i64, np: *const u8, nl: i64) {
    let (existing, new) = (read(ep, el), read(np, nl));
    if let Err(e) = std::fs::hard_link(&existing, &new) {
        throw_io(&e, "link", &existing);
    }
}

/// `fs.truncateSync(path, len)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_TRUNCATE(p: *const u8, l: i64, len: i64) {
    let path = read(p, l);
    let r = std::fs::OpenOptions::new().write(true).open(&path).and_then(|f| f.set_len(len.max(0) as u64));
    if let Err(e) = r {
        throw_io(&e, "open", &path);
    }
}

/// `fs.readdirSync(path)` → string[].
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_READDIR(p: *const u8, l: i64) -> u64 {
    let path = read(p, l);
    match std::fs::read_dir(&path) {
        Ok(rd) => {
            let names: Vec<String> = rd
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            string_array(&names)
        }
        Err(e) => {
            throw_io(&e, "scandir", &path);
            string_array(&[])
        }
    }
}

/// `fs.mkdtempSync(prefix)` → the created unique temp directory path. Node
/// appends 6 random `[A-Za-z0-9]` characters to `prefix`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_MKDTEMP(p: *const u8, l: i64) -> u64 {
    const ALPHABET: &[u8; 62] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let prefix = read(p, l);
    // Retry on the astronomically-unlikely collision; surface any other error.
    for _ in 0..64 {
        let mut rand = [0u8; 6];
        if getrandom::getrandom(&mut rand).is_err() {
            break;
        }
        let suffix: String = rand.iter().map(|&b| ALPHABET[(b % 62) as usize] as char).collect();
        let path = format!("{prefix}{suffix}");
        match std::fs::create_dir(&path) {
            Ok(()) => return intern(&path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                throw_io(&e, "mkdtemp", &path);
                return intern("");
            }
        }
    }
    intern("")
}

/// `fs.readlinkSync(path)` → the symlink's target path.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_READLINK(p: *const u8, l: i64) -> u64 {
    let path = read(p, l);
    match std::fs::read_link(&path) {
        Ok(target) => intern(&target.to_string_lossy()),
        Err(e) => {
            throw_io(&e, "readlink", &path);
            intern("")
        }
    }
}

/// `fs.realpathSync(path)` → string.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_REALPATH(p: *const u8, l: i64) -> u64 {
    let path = read(p, l);
    match std::fs::canonicalize(&path) {
        Ok(pb) => intern(&pb.to_string_lossy()),
        Err(e) => {
            throw_io(&e, "realpath", &path);
            intern("")
        }
    }
}

/// `fs.statSync(path)` → Stats (follows symlinks).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_STAT(p: *const u8, l: i64) -> u64 {
    let path = read(p, l);
    match std::fs::metadata(&path) {
        Ok(m) => stats::build(&m),
        Err(e) => {
            throw_io(&e, "stat", &path);
            0
        }
    }
}

/// `fs.lstatSync(path)` → Stats (does not follow symlinks).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_LSTAT(p: *const u8, l: i64) -> u64 {
    let path = read(p, l);
    match std::fs::symlink_metadata(&path) {
        Ok(m) => stats::build(&m),
        Err(e) => {
            throw_io(&e, "lstat", &path);
            0
        }
    }
}

// ---- Stats instance methods ----

/// `stats.isFile()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_STATS_IS_FILE(this: u64) -> i64 {
    stats::is_file(this) as i64
}

/// `stats.isDirectory()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_STATS_IS_DIRECTORY(this: u64) -> i64 {
    stats::is_directory(this) as i64
}

/// `stats.isSymbolicLink()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_STATS_IS_SYMLINK(this: u64) -> i64 {
    stats::is_symbolic_link(this) as i64
}

/// `stats.size` / `.mode` / `.mtimeMs` / `.atimeMs` / `.ctimeMs` / `.birthtimeMs`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_STATS_SIZE(this: u64) -> f64 {
    stats::num_field(this, "size")
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_STATS_MODE(this: u64) -> f64 {
    stats::num_field(this, "mode")
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_STATS_MTIME_MS(this: u64) -> f64 {
    stats::num_field(this, "mtimeMs")
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_STATS_ATIME_MS(this: u64) -> f64 {
    stats::num_field(this, "atimeMs")
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_STATS_CTIME_MS(this: u64) -> f64 {
    stats::num_field(this, "ctimeMs")
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_STATS_BIRTHTIME_MS(this: u64) -> f64 {
    stats::num_field(this, "birthtimeMs")
}
