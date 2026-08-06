//! `readFileSync`/`writeFileSync`/`appendFileSync`/`existsSync`/
//! `copyFileSync`/`renameSync`/`unlinkSync` — the members that read or write
//! whole-file contents by path, no fd and no directory tree involved.

use super::{bool_value, string, text};

/// `fs.readFileSync(path, encoding?)` — always TEXT, see the module doc.
///
/// `undefined` when the read fails, rather than `""`: an empty file and a
/// missing one must not read the same.
pub(super) extern "C" fn read_file_sync(_e: u64, _this: u64, path: u64, _encoding: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(path) = text(path) else {
        return rts_core_rwk::entry::undefined_value();
    };
    match std::fs::read_to_string(&path) {
        Ok(contents) => string(&contents),
        Err(_) => rts_core_rwk::entry::undefined_value(),
    }
}

/// `fs.writeFileSync(path, data)` — overwrites, creates if absent.
pub(super) extern "C" fn write_file_sync(_e: u64, _this: u64, path: u64, data: u64, _a2: u64, _a3: u64) -> u64 {
    if let (Some(path), Some(data)) = (text(path), text(data)) {
        let _ = std::fs::write(path, data);
    }
    rts_core_rwk::entry::undefined_value()
}

/// `fs.appendFileSync(path, data)`.
pub(super) extern "C" fn append_file_sync(_e: u64, _this: u64, path: u64, data: u64, _a2: u64, _a3: u64) -> u64 {
    use std::io::Write;
    if let (Some(path), Some(data)) = (text(path), text(data)) {
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = file.write_all(data.as_bytes());
        }
    }
    rts_core_rwk::entry::undefined_value()
}

/// `fs.existsSync(path)` — `false` for a missing path, and `false` for a path
/// that could not be checked for any other reason: Node's `existsSync` never
/// throws, so this is the one member that already had a real boolean answer
/// for every case.
pub(super) extern "C" fn exists_sync(_e: u64, _this: u64, path: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    bool_value(text(path).is_some_and(|path| std::path::Path::new(&path).exists()))
}

/// `fs.copyFileSync(src, dest)`.
pub(super) extern "C" fn copy_file_sync(_e: u64, _this: u64, src: u64, dest: u64, _a2: u64, _a3: u64) -> u64 {
    if let (Some(src), Some(dest)) = (text(src), text(dest)) {
        let _ = std::fs::copy(src, dest);
    }
    rts_core_rwk::entry::undefined_value()
}

/// `fs.renameSync(from, to)`.
pub(super) extern "C" fn rename_sync(_e: u64, _this: u64, from: u64, to: u64, _a2: u64, _a3: u64) -> u64 {
    if let (Some(from), Some(to)) = (text(from), text(to)) {
        let _ = std::fs::rename(from, to);
    }
    rts_core_rwk::entry::undefined_value()
}

/// `fs.unlinkSync(path)`.
pub(super) extern "C" fn unlink_sync(_e: u64, _this: u64, path: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    if let Some(path) = text(path) {
        let _ = std::fs::remove_file(path);
    }
    rts_core_rwk::entry::undefined_value()
}
