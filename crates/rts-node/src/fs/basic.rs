//! `readFileSync`/`writeFileSync`/`appendFileSync`/`existsSync`/
//! `copyFileSync`/`renameSync`/`unlinkSync` — the members that read or write
//! whole-file contents by path, no fd and no directory tree involved.

use super::{bool_value, encoding, string, text};

/// `fs.readFileSync(path, encoding?)` — text under `encoding` (`"utf8"` when
/// absent or unrecognised, matching every caller in this crate today).
///
/// `undefined` when the read fails, rather than `""`: an empty file and a
/// missing one must not read the same.
///
/// # The encoding used to be ignored
///
/// `"hex"`/`"base64"`/`"utf16le"` all fell through to `read_to_string`, which
/// answers UTF-8 text (or fails outright on bytes that are not valid UTF-8) —
/// so `readFileSync(f, "hex")` answered the FILE's raw text, never the hex
/// digits Node's own `"hex"` encoding means. [`encoding::encode`] is the fix,
/// shared with [`super::bytes::read_file_sync`]'s no-encoding form having
/// nothing to do with this bug: that form never claimed to honour an
/// encoding it was never given.
pub(super) extern "C" fn read_file_sync(_e: u64, _this: u64, path: u64, encoding_arg: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(path) = text(path) else {
        return rts_core::entry::undefined_value();
    };
    let result = std::fs::read(&path);
    super::record_io(&result);
    let Ok(bytes) = result else {
        return rts_core::entry::undefined_value();
    };
    let name = text(encoding_arg).unwrap_or_else(|| "utf8".to_string());
    match encoding::encode(&name, &bytes) {
        Some(text) => string(&text),
        None => rts_core::entry::undefined_value(),
    }
}

/// `fs.writeFileSync(path, data, encoding?)` — overwrites, creates if absent.
/// `data` is read under `encoding` (`"utf8"` when absent/unrecognised) before
/// the write — see [`read_file_sync`]'s doc for the bug this and
/// [`append_file_sync`] share the fix with: a hex/base64/utf16le string
/// used to be written as its own literal UTF-8 bytes instead of what it names.
pub(super) extern "C" fn write_file_sync(_e: u64, _this: u64, path: u64, data: u64, encoding_arg: u64, _a3: u64) -> u64 {
    if let (Some(path), Some(data)) = (text(path), text(data)) {
        let name = text(encoding_arg).unwrap_or_else(|| "utf8".to_string());
        match encoding::decode(&name, &data) {
            Some(bytes) => super::record_io(&std::fs::write(path, bytes)),
            None => super::record(false),
        }
    }
    rts_core::entry::undefined_value()
}

/// `fs.appendFileSync(path, data, encoding?)`.
pub(super) extern "C" fn append_file_sync(_e: u64, _this: u64, path: u64, data: u64, encoding_arg: u64, _a3: u64) -> u64 {
    use std::io::Write;
    if let (Some(path), Some(data)) = (text(path), text(data)) {
        let name = text(encoding_arg).unwrap_or_else(|| "utf8".to_string());
        match encoding::decode(&name, &data) {
            Some(bytes) => match std::fs::OpenOptions::new().create(true).append(true).open(path) {
                Ok(mut file) => super::record_io(&file.write_all(&bytes)),
                Err(error) => super::record_io::<()>(&Err(error)),
            },
            None => super::record(false),
        }
    }
    rts_core::entry::undefined_value()
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
        super::record_io(&std::fs::copy(src, dest));
    }
    rts_core::entry::undefined_value()
}

/// `fs.renameSync(from, to)`.
pub(super) extern "C" fn rename_sync(_e: u64, _this: u64, from: u64, to: u64, _a2: u64, _a3: u64) -> u64 {
    if let (Some(from), Some(to)) = (text(from), text(to)) {
        super::record_io(&std::fs::rename(from, to));
    }
    rts_core::entry::undefined_value()
}

/// `fs.unlinkSync(path)`.
pub(super) extern "C" fn unlink_sync(_e: u64, _this: u64, path: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    if let Some(path) = text(path) {
        super::record_io(&std::fs::remove_file(path));
    }
    rts_core::entry::undefined_value()
}
