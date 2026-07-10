//! node:fs — base extern "C" symbol implementations (the sync surface).

use std::fs::{Metadata, OpenOptions};
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use rts_engine::abi::str_abi::from_abi;
use rts_engine::heap::handles::{alloc_entry, Entry};
use rts_engine::heap::shapes::{alloc_shaped_object, bool_word};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

/// Interns a Rust string as a GC string handle (the ABI `Handle` return).
fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// `fs.existsSync(path)` — true iff `path` exists (file, dir, or other).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_EXISTS_SYNC(path_ptr: *const u8, path_len: i64) -> i64 {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return 0;
    };
    if std::path::Path::new(path).exists() { 1 } else { 0 }
}

/// `fs.readFileSync(path)` — UTF-8 text variant. 0 on any error (missing
/// file, permission denied, invalid UTF-8 — `std::fs::read_to_string` folds
/// all three into one `Result`, matching a fail-soft ABI with no exceptions).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_READ_FILE_SYNC(path_ptr: *const u8, path_len: i64) -> u64 {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return 0;
    };
    match std::fs::read_to_string(path) {
        Ok(s) => intern(&s),
        Err(_) => 0,
    }
}

/// `fs.writeFileSync(path, data)` — creates or truncates `path` and writes
/// `data`. Errors are swallowed (void ABI has no error channel); Node throws,
/// this fails soft rather than panicking on bad input.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_WRITE_FILE_SYNC(
    path_ptr: *const u8,
    path_len: i64,
    data_ptr: *const u8,
    data_len: i64,
) {
    let (Some(path), Some(data)) =
        (unsafe { from_abi(path_ptr, path_len) }, unsafe { from_abi(data_ptr, data_len) })
    else {
        return;
    };
    let _ = std::fs::write(path, data.as_bytes());
}

/// `fs.appendFileSync(path, data)` — creates `path` if missing, appends
/// `data` otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_APPEND_FILE_SYNC(
    path_ptr: *const u8,
    path_len: i64,
    data_ptr: *const u8,
    data_len: i64,
) {
    let (Some(path), Some(data)) =
        (unsafe { from_abi(path_ptr, path_len) }, unsafe { from_abi(data_ptr, data_len) })
    else {
        return;
    };
    if let Ok(mut file) = OpenOptions::new().append(true).create(true).open(path) {
        let _ = file.write_all(data.as_bytes());
    }
}

/// `fs.mkdirSync(path)` — **non-recursive**: the parent must already exist
/// (mirrors `std::fs::create_dir`). Node's `{ recursive: true }` option is
/// deferred (needs an options object).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_MKDIR_SYNC(path_ptr: *const u8, path_len: i64) {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return;
    };
    let _ = std::fs::create_dir(path);
}

/// `fs.rmdirSync(path)` — removes an *empty* directory. Node's recursive
/// `{ recursive: true }` form (deprecated in Node itself in favor of
/// `rmSync`) is deferred (needs an options object).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_RMDIR_SYNC(path_ptr: *const u8, path_len: i64) {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return;
    };
    let _ = std::fs::remove_dir(path);
}

/// `fs.unlinkSync(path)` — removes a file.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_UNLINK_SYNC(path_ptr: *const u8, path_len: i64) {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return;
    };
    let _ = std::fs::remove_file(path);
}

/// `fs.renameSync(oldPath, newPath)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_RENAME_SYNC(
    old_ptr: *const u8,
    old_len: i64,
    new_ptr: *const u8,
    new_len: i64,
) {
    let (Some(old_path), Some(new_path)) =
        (unsafe { from_abi(old_ptr, old_len) }, unsafe { from_abi(new_ptr, new_len) })
    else {
        return;
    };
    let _ = std::fs::rename(old_path, new_path);
}

/// `fs.copyFileSync(src, dest)` — copies file contents (and permission bits,
/// via `std::fs::copy`), overwriting `dest`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_COPY_FILE_SYNC(
    src_ptr: *const u8,
    src_len: i64,
    dest_ptr: *const u8,
    dest_len: i64,
) {
    let (Some(src), Some(dest)) =
        (unsafe { from_abi(src_ptr, src_len) }, unsafe { from_abi(dest_ptr, dest_len) })
    else {
        return;
    };
    let _ = std::fs::copy(src, dest);
}

/// `fs.readdirSync(path)` — entry names only (`file_name()`, matching Node's
/// default with no `withFileTypes`/`recursive` options). 0 on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_READDIR_SYNC(path_ptr: *const u8, path_len: i64) -> u64 {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return 0;
    };
    let Ok(iter) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut entries: Vec<i64> = Vec::new();
    for entry in iter.flatten() {
        let name = entry.file_name();
        if let Some(s) = name.to_str() {
            // Element as a string WORD (new engine), not a raw handle.
            entries.push(rts_engine::heap::shapes::string_word(s.as_bytes()) as i64);
        }
    }
    alloc_entry(Entry::Vec(Box::new(entries)))
}

/// `fs.realpathSync(path)` — resolves symlinks/`.`/`..` against the real
/// filesystem (`std::fs::canonicalize`). 0 on error (missing path, or a
/// resolved path that is not valid UTF-8).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_REALPATH_SYNC(path_ptr: *const u8, path_len: i64) -> u64 {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return 0;
    };
    match std::fs::canonicalize(path) {
        Ok(resolved) => resolved.to_str().map(intern).unwrap_or(0),
        Err(_) => 0,
    }
}

/// `fs.accessSync(path)` — performs the real existence check
/// (`std::fs::metadata`) Node's `accessSync` is built on. **Caveat:** Node
/// throws on failure (missing path / permission denied) so callers can
/// `try/catch`; the `void` ABI here has no error channel to report that
/// through, so this always returns without signaling success/failure — it
/// still does the genuine syscall (no faked pass), the result is just not
/// observable from this call alone (pair with `existsSync` to check first).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_ACCESS_SYNC(path_ptr: *const u8, path_len: i64) {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return;
    };
    let _ = std::fs::metadata(path);
}

/// `fs.truncateSync(path, len)` — opens `path` for writing and sets its size
/// to `len` bytes (`File::set_len`), matching `ftruncate` semantics: shorter
/// truncates, longer zero-extends. `len` is clamped to `>= 0` (a negative or
/// NaN `len` has no valid byte-length meaning; treated as `0`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_TRUNCATE_SYNC(path_ptr: *const u8, path_len: i64, len: f64) {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return;
    };
    let new_len = if len.is_finite() && len > 0.0 { len as u64 } else { 0 };
    if let Ok(file) = OpenOptions::new().write(true).open(path) {
        let _ = file.set_len(new_len);
    }
}

/// `fs.readlinkSync(path)` — resolves the target of the symlink at `path`
/// (`std::fs::read_link`), returned as-is (not further canonicalized, matching
/// Node). 0 on error (not a symlink, missing, or a target not valid UTF-8).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_READLINK_SYNC(path_ptr: *const u8, path_len: i64) -> u64 {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return 0;
    };
    match std::fs::read_link(path) {
        Ok(target) => target.to_str().map(intern).unwrap_or(0),
        Err(_) => 0,
    }
}

/// `fs.rmSync(path)` — removes a file OR an empty directory. Tries
/// `std::fs::remove_file` first; if that fails (e.g. `path` is a directory,
/// or a permissions error that a directory-removal might still clear), falls
/// back to `std::fs::remove_dir` (**non-recursive** — Node's
/// `{ recursive: true }` option, which would also remove non-empty
/// directories, is deferred: needs an options object).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_RM_SYNC(path_ptr: *const u8, path_len: i64) {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return;
    };
    if std::fs::remove_file(path).is_err() {
        let _ = std::fs::remove_dir(path);
    }
}

/// Fills `out` with `out.len()` random lowercase-alphanumeric ASCII chars
/// (`[a-z0-9]`, 36 symbols) sourced from OS entropy (`getrandom`). Used by
/// `mkdtempSync` to build the unique suffix Node appends to the prefix.
/// Returns `false` if the OS entropy call itself failed.
fn random_lowercase_alnum(out: &mut [u8]) -> bool {
    const ALPHABET: &[u8; 36] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    if getrandom::getrandom(out).is_err() {
        return false;
    }
    for b in out.iter_mut() {
        *b = ALPHABET[(*b as usize) % ALPHABET.len()];
    }
    true
}

/// `fs.mkdtempSync(prefix)` — creates a new unique temp directory named
/// `prefix` + 6 random lowercase-alphanumeric chars (Node appends 6 random
/// characters to the given prefix; the OS-entropy-backed selection here plays
/// the role of Node's own random suffix generator), via
/// `std::fs::create_dir`. Returns the full created path, or 0 on error
/// (entropy failure, or `create_dir` failure — e.g. parent directory of
/// `prefix` missing, or the astronomically unlikely name collision).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_MKDTEMP_SYNC(prefix_ptr: *const u8, prefix_len: i64) -> u64 {
    let Some(prefix) = (unsafe { from_abi(prefix_ptr, prefix_len) }) else {
        return 0;
    };
    let mut suffix = [0u8; 6];
    if !random_lowercase_alnum(&mut suffix) {
        return 0;
    }
    // ALPHABET is pure ASCII, so this is always valid UTF-8.
    let suffix = std::str::from_utf8(&suffix).unwrap();
    let full_path = format!("{prefix}{suffix}");
    match std::fs::create_dir(&full_path) {
        Ok(()) => intern(&full_path),
        Err(_) => 0,
    }
}

/// `fs.mkdirpSync(path)` — creates `path` AND every missing parent directory
/// (`std::fs::create_dir_all`), mirroring Node's
/// `mkdirSync(path, { recursive: true })`. No-op (success) if `path` already
/// exists as a directory.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_MKDIRP_SYNC(path_ptr: *const u8, path_len: i64) {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return;
    };
    let _ = std::fs::create_dir_all(path);
}

/// A `SystemTime` field from `Metadata` (`modified`/`accessed`/`created`,
/// each `io::Result<SystemTime>` — not every platform/filesystem reports
/// every one) as Node's `*Ms` millisecond-since-epoch number. `0` when the
/// platform doesn't report the field (matches the `statSync`/`lstatSync`
/// doc contract — never a faked non-zero value).
fn systemtime_to_ms(t: std::io::Result<SystemTime>) -> f64 {
    t.ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

/// `ctimeMs` (inode/metadata change time). Unix has a genuine, distinct
/// `st_ctime` (`MetadataExt::ctime`/`ctime_nsec`, real nanosecond precision);
/// non-Unix platforms (Windows/NTFS) have no such concept, so — matching
/// Node's OWN libuv shim, which sets `ctim = mtim` when the platform has no
/// change-time field — this falls back to `mtime`.
#[cfg(unix)]
fn ctime_ms(meta: &Metadata) -> f64 {
    use std::os::unix::fs::MetadataExt;
    meta.ctime() as f64 * 1000.0 + meta.ctime_nsec() as f64 / 1_000_000.0
}
#[cfg(not(unix))]
fn ctime_ms(meta: &Metadata) -> f64 {
    systemtime_to_ms(meta.modified())
}

/// POSIX-style `mode` bits. Unix: the REAL `st_mode` from the syscall
/// (`MetadataExt::mode`, type bits + permission bits already combined).
/// Windows has no POSIX mode bits at the syscall level, so this mirrors
/// Node's OWN translation (libuv `fs__file_info_to_stat`): file-type bits
/// (`S_IFLNK`/`S_IFDIR`/`S_IFREG`) OR'd with `0o444` (read-only attribute
/// set) or `0o666` (writable) — real, derived from the actual
/// `FILE_ATTRIBUTE_READONLY` bit, not a hardcoded constant; Windows has no
/// execute-bit concept, matching Node's own output here.
#[cfg(unix)]
fn stat_mode(meta: &Metadata) -> f64 {
    use std::os::unix::fs::MetadataExt;
    meta.mode() as f64
}
#[cfg(windows)]
fn stat_mode(meta: &Metadata) -> f64 {
    let readonly = meta.permissions().readonly();
    let type_bits: u32 = if meta.file_type().is_symlink() {
        0o120_000
    } else if meta.is_dir() {
        0o040_000
    } else {
        0o100_000
    };
    let perm_bits: u32 = if readonly { 0o444 } else { 0o666 };
    (type_bits | perm_bits) as f64
}
#[cfg(not(any(unix, windows)))]
fn stat_mode(_meta: &Metadata) -> f64 {
    0.0
}

/// Builds the `Stats`-shaped DATA object `statSync`/`lstatSync` (and
/// `fs/promises.stat`, which reuses this) return, from a real
/// `std::fs::Metadata`. **Node exposes `isFile()`/`isDirectory()`/
/// `isSymbolicLink()` as METHODS** (fn-valued properties) — this slice has
/// no fn-in-object-literal machinery, so the boolean is exposed as plain
/// DATA under the `*Value` suffix (`isFileValue` etc.) instead; a caller
/// wanting the exact Node API shape needs a thin `.ts` wrapper
/// (`isFile() { return this.isFileValue }`) layered over this, out of scope
/// for this native slice.
pub(crate) fn build_stats_object(meta: &Metadata) -> u64 {
    let size = meta.len() as f64;
    let mtime_ms = systemtime_to_ms(meta.modified());
    let atime_ms = systemtime_to_ms(meta.accessed());
    let birthtime_ms = systemtime_to_ms(meta.created());
    let ctime_ms_v = ctime_ms(meta);
    let mode = stat_mode(meta);
    let is_file = meta.is_file();
    let is_dir = meta.is_dir();
    let is_symlink = meta.file_type().is_symlink();

    let keys: &[&str] = &[
        "size",
        "mtimeMs",
        "atimeMs",
        "ctimeMs",
        "birthtimeMs",
        "mode",
        "isFileValue",
        "isDirectoryValue",
        "isSymbolicLinkValue",
    ];
    let values: [i64; 9] = [
        size.to_bits() as i64,
        mtime_ms.to_bits() as i64,
        atime_ms.to_bits() as i64,
        ctime_ms_v.to_bits() as i64,
        birthtime_ms.to_bits() as i64,
        mode.to_bits() as i64,
        bool_word(is_file) as i64,
        bool_word(is_dir) as i64,
        bool_word(is_symlink) as i64,
    ];
    alloc_shaped_object(keys, &values)
}

/// `fs.statSync(path)` — follows symlinks (`std::fs::metadata`), matching
/// Node's `statSync`. Returns a `Stats`-shaped object (see
/// [`build_stats_object`]); `0` on error (missing path, permission denied).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_STAT_SYNC(path_ptr: *const u8, path_len: i64) -> u64 {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return 0;
    };
    match std::fs::metadata(path) {
        Ok(meta) => build_stats_object(&meta),
        Err(_) => 0,
    }
}

/// `fs.lstatSync(path)` — does NOT follow symlinks
/// (`std::fs::symlink_metadata`), matching Node's `lstatSync`: for a
/// symlink, `isSymbolicLinkValue` is true and the other fields describe the
/// LINK itself, not its target. `0` on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_LSTAT_SYNC(path_ptr: *const u8, path_len: i64) -> u64 {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return 0;
    };
    match std::fs::symlink_metadata(path) {
        Ok(meta) => build_stats_object(&meta),
        Err(_) => 0,
    }
}
