//! node:fs — the synchronous `extern "C"` entry points, each a thin wrapper over
//! `std::fs` that throws a Node-style error on failure. No fabricated results.

use super::stats;
use super::words::{byte_array, intern, opt_bool, read, read_bytes, string_array, throw_io};

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

/// `fs.readFileSync(path, encoding)` → string.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_READ_FILE_ENC(p: *const u8, l: i64, _ep: *const u8, _el: i64) -> u64 {
    let path = read(p, l);
    match std::fs::read(&path) {
        Ok(bytes) => intern(&String::from_utf8_lossy(&bytes)),
        Err(e) => {
            throw_io(&e, "open", &path);
            intern("")
        }
    }
}

/// `fs.writeFileSync(path, data)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_WRITE_FILE(p: *const u8, l: i64, data: u64) {
    let path = read(p, l);
    if let Err(e) = std::fs::write(&path, read_bytes(data)) {
        throw_io(&e, "open", &path);
    }
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

/// `fs.copyFileSync(src, dest)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_COPY_FILE(sp: *const u8, sl: i64, dp: *const u8, dl: i64) {
    let (src, dest) = (read(sp, sl), read(dp, dl));
    if let Err(e) = std::fs::copy(&src, &dest) {
        throw_io(&e, "copyfile", &src);
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
