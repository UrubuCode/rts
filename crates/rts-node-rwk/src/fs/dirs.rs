//! `mkdirSync`/`rmSync`/`rmdirSync`/`readdirSync`/`mkdtempSync`/`cpSync` — the
//! members that operate on a directory tree rather than a single file.

use super::{option_flag, string, text};

/// `fs.mkdirSync(path, { recursive })`.
///
/// The options object is read with `get_member`, not positionally: Node's
/// second argument is an options object or a mode number, and only the shape
/// this crate needs — `recursive` — is read from it.
pub(super) extern "C" fn mkdir_sync(_e: u64, _this: u64, path: u64, options: u64, _a2: u64, _a3: u64) -> u64 {
    if let Some(path) = text(path) {
        let recursive = option_flag(options, "recursive");
        let _ = match recursive {
            true => std::fs::create_dir_all(path),
            false => std::fs::create_dir(path),
        };
    }
    rts_core_rwk::entry::undefined_value()
}

/// `fs.rmSync(path, { recursive })`.
pub(super) extern "C" fn rm_sync(_e: u64, _this: u64, path: u64, options: u64, _a2: u64, _a3: u64) -> u64 {
    if let Some(path) = text(path) {
        let recursive = option_flag(options, "recursive");
        let target = std::path::Path::new(&path);
        let _ = match (recursive, target.is_dir()) {
            (true, true) => std::fs::remove_dir_all(target),
            _ => std::fs::remove_file(target),
        };
    }
    rts_core_rwk::entry::undefined_value()
}

/// `fs.rmdirSync(path)` — an empty directory only; `rmSync` is where
/// `recursive` lives, matching Node's own split between the two.
pub(super) extern "C" fn rmdir_sync(_e: u64, _this: u64, path: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    if let Some(path) = text(path) {
        let _ = std::fs::remove_dir(path);
    }
    rts_core_rwk::entry::undefined_value()
}

/// `fs.readdirSync(path)` — an array of entry names.
///
/// `undefined` on failure, matching `readFileSync`'s stand-in rather than an
/// empty array: a missing directory and an empty one must not read the same.
///
/// `withFileTypes`/`recursive` are not read from `options` — the first needs
/// `Dirent` objects this module does not build (see the module doc), and
/// this is the plain flat-name form Node calls the default.
pub(super) extern "C" fn readdir_sync(_e: u64, _this: u64, path: u64, _options: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(path) = text(path) else {
        return rts_core_rwk::entry::undefined_value();
    };
    match std::fs::read_dir(&path) {
        Ok(entries) => {
            let names: Vec<u64> = entries
                .filter_map(|entry| entry.ok())
                .map(|entry| string(&entry.file_name().to_string_lossy()))
                .collect();
            rts_core_rwk::entry::make_array(names)
        }
        Err(_) => rts_core_rwk::entry::undefined_value(),
    }
}

/// `fs.mkdtempSync(prefix)` — `prefix` plus 6 pseudo-random characters,
/// retried on collision.
///
/// Node uses a CSPRNG; this uses the system clock's nanosecond field, which
/// this module may use without adding a dependency and is unique enough for
/// a same-process temp-name generator — cryptographic unpredictability is
/// not a property `mkdtemp` promises.
pub(super) extern "C" fn mkdtemp_sync(_e: u64, _this: u64, prefix: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(prefix) = text(prefix) else {
        return rts_core_rwk::entry::undefined_value();
    };
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    for attempt in 0..64u64 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64)
            .unwrap_or(0)
            .wrapping_add(attempt.wrapping_mul(2_654_435_761));
        let mut suffix = String::with_capacity(6);
        let mut bits = nanos;
        for _ in 0..6 {
            suffix.push(ALPHABET[(bits % ALPHABET.len() as u64) as usize] as char);
            bits /= ALPHABET.len() as u64;
        }
        let candidate = format!("{prefix}{suffix}");
        if std::fs::create_dir(&candidate).is_ok() {
            return string(&candidate);
        }
    }
    rts_core_rwk::entry::undefined_value()
}

/// `fs.cpSync(src, dest, { recursive })`.
///
/// Only `recursive` is read from `options` — `filter`/`dereference`/
/// `errorOnExist`/`preserveTimestamps`/`verbatimSymlinks` all need either a
/// user callback (re-entry this module cannot do) or a symlink/timestamp API
/// this module does not have (see the module doc); a non-recursive `cpSync`
/// on a file is exactly `copyFileSync`'s behaviour.
pub(super) extern "C" fn cp_sync(_e: u64, _this: u64, src: u64, dest: u64, options: u64, _a3: u64) -> u64 {
    if let (Some(src), Some(dest)) = (text(src), text(dest)) {
        let recursive = option_flag(options, "recursive");
        let src = std::path::Path::new(&src);
        let dest = std::path::Path::new(&dest);
        if recursive && src.is_dir() {
            let _ = copy_dir_all(src, dest);
        } else {
            let _ = std::fs::copy(src, dest);
        }
    }
    rts_core_rwk::entry::undefined_value()
}

/// The recursive walk `cpSync` needs and `std::fs` has no primitive for.
fn copy_dir_all(src: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
