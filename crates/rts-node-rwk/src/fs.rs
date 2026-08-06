//! `node:fs` — the SYNCHRONOUS surface, over `std::fs`.
//!
//! # Why synchronous only
//!
//! Every async form in real `node:fs` answers a `Promise` or takes a callback,
//! and both are user-code re-entry this crate's natives cannot do — an
//! `extern "C"` frame here cannot call back into compiled code without a
//! borrow of the runtime it must not hold across that call (see
//! `crates/rts-host-rwk/PLAN.md`). The `Sync` suffix forms have no such
//! obligation: they run to completion and answer a value, which is exactly
//! what a native can do.
//!
//! # `readFileSync` and the missing `Buffer`
//!
//! Node's `readFileSync(p)` with no encoding answers a `Buffer`. This engine
//! has none, so this DIVERGES: `readFileSync(p)` and `readFileSync(p, "utf8")`
//! both answer the file's contents as a TEXT string. A caller relying on
//! `Buffer`-specific behaviour (binary contents, `.length` in bytes over
//! non-UTF-8 data) gets something else here, named rather than approximated —
//! there is no Buffer-like object standing in for it.
//!
//! # Errors: no throw, a stated stand-in
//!
//! Node throws on a missing file, a permission failure, and so on. A native
//! entry point here cannot throw across the call boundary, so each member
//! states its OWN stand-in rather than sharing one:
//! - a read that fails (`readFileSync`) answers `undefined` — never `""`,
//!   which would be a wrong answer that runs silently.
//! - an operation whose real answer is boolean (`existsSync`) answers `false`.
//! - an operation with no other value to give (`writeFileSync`,
//!   `appendFileSync`, `mkdirSync`, `rmSync`, `copyFileSync`, `renameSync`,
//!   `unlinkSync`, `rmdirSync`) answers `undefined` on success AND on failure
//!   alike; a caller that needs to know which happened has nothing here to
//!   ask, which is the gap to name rather than paper over.
//!
//! # Not implemented, by name
//!
//! Every async/Promise form (`readFile`, `writeFile`, `fs.promises.*`, …),
//! every callback form, `watch`/`watchFile` and all of `FSWatcher`, streams
//! (`createReadStream`, `createWriteStream`), file descriptors and everything
//! built on one (`open`, `read`, `write`, `close`, `fstatSync`, …),
//! permission and ownership calls (`chmodSync`, `chownSync`, `accessSync`),
//! links (`symlinkSync`, `readlinkSync`, `linkSync`), `realpathSync`,
//! `truncateSync`, `utimesSync`, `Dirent`/`opendirSync`, and `constants`.

use rts_core_rwk::entry::{Context, Provided};

/// The namespace `node:fs` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("readFileSync", read_file_sync),
        ("writeFileSync", write_file_sync),
        ("appendFileSync", append_file_sync),
        ("existsSync", exists_sync),
        ("mkdirSync", mkdir_sync),
        ("rmSync", rm_sync),
        ("readdirSync", readdir_sync),
        ("statSync", stat_sync),
        ("copyFileSync", copy_file_sync),
        ("renameSync", rename_sync),
        ("unlinkSync", unlink_sync),
        ("rmdirSync", rmdir_sync),
    ];
    rts_core_rwk::entry::make_namespace(context, members)
}

/// `fs.readFileSync(path, encoding?)` — always TEXT, see the module doc.
///
/// `undefined` when the read fails, rather than `""`: an empty file and a
/// missing one must not read the same.
extern "C" fn read_file_sync(_e: u64, _this: u64, path: u64, _encoding: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(path) = text(path) else {
        return rts_core_rwk::entry::undefined_value();
    };
    match std::fs::read_to_string(&path) {
        Ok(contents) => string(&contents),
        Err(_) => rts_core_rwk::entry::undefined_value(),
    }
}

/// `fs.writeFileSync(path, data)` — overwrites, creates if absent.
extern "C" fn write_file_sync(_e: u64, _this: u64, path: u64, data: u64, _a2: u64, _a3: u64) -> u64 {
    if let (Some(path), Some(data)) = (text(path), text(data)) {
        let _ = std::fs::write(path, data);
    }
    rts_core_rwk::entry::undefined_value()
}

/// `fs.appendFileSync(path, data)`.
extern "C" fn append_file_sync(_e: u64, _this: u64, path: u64, data: u64, _a2: u64, _a3: u64) -> u64 {
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
extern "C" fn exists_sync(_e: u64, _this: u64, path: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    bool_value(text(path).is_some_and(|path| std::path::Path::new(&path).exists()))
}

/// `fs.mkdirSync(path, { recursive })`.
///
/// The options object is read with `get_member`, not positionally: Node's
/// second argument is an options object or a mode number, and only the shape
/// this crate needs — `recursive` — is read from it.
extern "C" fn mkdir_sync(_e: u64, _this: u64, path: u64, options: u64, _a2: u64, _a3: u64) -> u64 {
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
extern "C" fn rm_sync(_e: u64, _this: u64, path: u64, options: u64, _a2: u64, _a3: u64) -> u64 {
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

/// `fs.readdirSync(path)` — an array of entry names.
///
/// `undefined` on failure, matching `readFileSync`'s stand-in rather than an
/// empty array: a missing directory and an empty one must not read the same.
extern "C" fn readdir_sync(_e: u64, _this: u64, path: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
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

/// `fs.statSync(path)` — an object with `size`, `isFile()`, `isDirectory()`.
///
/// `undefined` on failure. `isFile`/`isDirectory` are built as callables over
/// a fixed boolean rather than plain properties, matching Node's own
/// `fs.Stats` shape, where they are methods.
///
/// `size` is a NUMBER, which it was not for the length of one review: the
/// host-facing surface had no numeric constructor, so the first version of this
/// answered a decimal string and said so rather than smuggling a byte count
/// through as some other type. `make_number` exists because of that report — the
/// gap was in the API this module was given, not in the module.
extern "C" fn stat_sync(_e: u64, _this: u64, path: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(path) = text(path) else {
        return rts_core_rwk::entry::undefined_value();
    };
    let Ok(metadata) = std::fs::metadata(&path) else {
        return rts_core_rwk::entry::undefined_value();
    };
    let is_file = metadata.is_file();
    let is_dir = metadata.is_dir();
    let size = metadata.len();
    rts_core_rwk::entry::with_runtime(|context| {
        let stats = rts_core_rwk::entry::make_object(context);
        let size_value = rts_core_rwk::entry::make_number(size as f64);
        rts_core_rwk::entry::put_member(context, stats, "size", size_value);
        let is_file_fn = rts_core_rwk::entry::make_callable(context, is_file_answer(is_file));
        rts_core_rwk::entry::put_member(context, stats, "isFile", is_file_fn);
        let is_dir_fn = rts_core_rwk::entry::make_callable(context, is_dir_answer(is_dir));
        rts_core_rwk::entry::put_member(context, stats, "isDirectory", is_dir_fn);
        stats
    })
}

/// Picks the zero-argument native that answers a fixed `isFile()` result.
///
/// Two functions rather than one closing over a captured bool: a
/// `Provided`/`Native` here is a bare `extern "C" fn`, with no room for
/// captured state, so the two possible answers are two statically distinct
/// functions instead.
fn is_file_answer(value: bool) -> Provided {
    match value {
        true => is_file_true,
        false => is_file_false,
    }
}

/// Picks the zero-argument native that answers a fixed `isDirectory()` result.
fn is_dir_answer(value: bool) -> Provided {
    match value {
        true => is_dir_true,
        false => is_dir_false,
    }
}

extern "C" fn is_file_true(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    rts_core_rwk::entry::boolean_value(true)
}
extern "C" fn is_file_false(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    rts_core_rwk::entry::boolean_value(false)
}
extern "C" fn is_dir_true(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    rts_core_rwk::entry::boolean_value(true)
}
extern "C" fn is_dir_false(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    rts_core_rwk::entry::boolean_value(false)
}

/// `fs.copyFileSync(src, dest)`.
extern "C" fn copy_file_sync(_e: u64, _this: u64, src: u64, dest: u64, _a2: u64, _a3: u64) -> u64 {
    if let (Some(src), Some(dest)) = (text(src), text(dest)) {
        let _ = std::fs::copy(src, dest);
    }
    rts_core_rwk::entry::undefined_value()
}

/// `fs.renameSync(from, to)`.
extern "C" fn rename_sync(_e: u64, _this: u64, from: u64, to: u64, _a2: u64, _a3: u64) -> u64 {
    if let (Some(from), Some(to)) = (text(from), text(to)) {
        let _ = std::fs::rename(from, to);
    }
    rts_core_rwk::entry::undefined_value()
}

/// `fs.unlinkSync(path)`.
extern "C" fn unlink_sync(_e: u64, _this: u64, path: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    if let Some(path) = text(path) {
        let _ = std::fs::remove_file(path);
    }
    rts_core_rwk::entry::undefined_value()
}

/// `fs.rmdirSync(path)` — an empty directory only; `rmSync` is where
/// `recursive` lives, matching Node's own split between the two.
extern "C" fn rmdir_sync(_e: u64, _this: u64, path: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    if let Some(path) = text(path) {
        let _ = std::fs::remove_dir(path);
    }
    rts_core_rwk::entry::undefined_value()
}

/// A boolean option read off an options object, `false` when the argument is
/// absent, not an object, or does not carry the key.
fn option_flag(options: u64, name: &str) -> bool {
    let absent = rts_core_rwk::entry::undefined_value();
    if options == absent {
        return false;
    }
    let value = rts_core_rwk::entry::with_runtime(|context| {
        rts_core_rwk::entry::get_member(context, options, name)
    });
    value == rts_core_rwk::entry::boolean_value(true)
}

/// An argument as text, `None` when absent.
fn text(value: u64) -> Option<String> {
    let absent = rts_core_rwk::entry::undefined_value();
    match value == absent {
        true => None,
        false => rts_core_rwk::entry::text_of(value),
    }
}

/// A string value.
fn string(text: &str) -> u64 {
    rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_string(context, text))
}

/// A boolean value.
fn bool_value(held: bool) -> u64 {
    rts_core_rwk::entry::boolean_value(held)
}
