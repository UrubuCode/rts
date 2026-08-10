//! `statSync`/`lstatSync` — `Stats` instances over one shared prototype and
//! `std::fs::Metadata`.
//!
//! `fstatSync` (the fd-based form) lives in [`super::fd`] beside the fd table
//! it needs, and calls [`build`] from here rather than duplicating it.
//!
//! # One prototype, not a closure per instance
//!
//! Every earlier version of this module hung `isFile`/`isDirectory`/
//! `isSymbolicLink` directly on each instance as one of two statically
//! distinct zero-argument natives, chosen by the boolean at build time — a
//! [`Provided`] is a bare `extern "C" fn` with no captured-state slot, so a
//! closure over a bool could not exist and two fixed functions stood in for
//! one. That gave every `Stats` its OWN methods: `Object.keys(stats)` listed
//! them, and two `Stats` objects shared no prototype under
//! `Object.getPrototypeOf`.
//!
//! [`entry::make_prototype`]/[`entry::make_instance`] fix both: one object
//! holds every predicate, made once and remembered by name, and each instance
//! only carries its OWN data. The predicates read a hidden `__type` bitmask
//! rather than a captured bool — the exact tagging [`super::dirent::type_bits`]
//! computes, reused rather than re-derived, so a `Stats` and a `Dirent` built
//! from the same `std::fs::FileType` never disagree about what it was.
//!
//! # `Date` fields: not built, named rather than approximated
//!
//! The reference doc lists `atime`/`mtime`/`ctime`/`birthtime` as `Date`
//! objects alongside their `*Ms` millisecond twins. `rts_core_rwk::entry`
//! exposes no constructor a host can call to build a `Date` value — the class
//! exists (`crates/rts-core-rwk/src/entry/date/mod.rs`) but nothing in the
//! host-facing surface this module was given reaches it. So only the `*Ms`
//! numbers are built; the `Date` forms are refused BY NAME rather than stood
//! in for with a plain object that merely looks like one.
//!
//! # Per-field honesty, not a plausible zero
//!
//! `dev`/`ino`/`mode`/`nlink`/`uid`/`gid`/`rdev`/`blksize`/`blocks` need
//! `std::os::unix::fs::MetadataExt` on Unix, or a synthesis from
//! `std::os::windows::fs::MetadataExt::file_attributes()` on Windows (see
//! [`windows_numeric`] for what that synthesis is and why it is not a guess —
//! it is `libuv`'s own `uv__stat`, which is what real Node's `fs.statSync`
//! runs through on that platform). On any OTHER target every one of those is
//! `undefined`, not `0`, because `0` is a legitimate device/inode number on
//! some systems and would read as an answer rather than as "not available
//! here". `ctimeMs` is the same story one field at a time: Unix has a real
//! change time (`MetadataExt::ctime`/`ctime_nsec`); `std::fs::Metadata` has
//! no cross-platform accessor for it at all, so it is `undefined` everywhere
//! else, Windows included — Windows has no separate change-time concept to
//! report even synthetically. `atimeMs`/`mtimeMs`/`birthtimeMs` come from
//! `Metadata::accessed`/`modified`/`created`, which exist on every target but
//! answer `Err` where the underlying filesystem does not track that
//! timestamp (Linux `birthtime` on some filesystems is the common case) —
//! `undefined` there too, learned from the `Result` rather than guessed.

use rts_core_rwk::entry::{self, Context, Provided};

use super::dirent::type_bits;

const METHODS: &[(&str, Provided)] = &[
    ("isFile", is_file),
    ("isDirectory", is_directory),
    ("isSymbolicLink", is_symbolic_link),
    ("isBlockDevice", is_block_device),
    ("isCharacterDevice", is_character_device),
    ("isFIFO", is_fifo),
    ("isSocket", is_socket),
];

fn type_of(this: u64) -> u32 {
    let value = entry::get_indexed(this, super::string("__type"));
    entry::number_of(value).unwrap_or(0.0) as u32
}

macro_rules! predicate {
    ($name:ident, $bit:expr) => {
        extern "C" fn $name(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
            entry::boolean_value(type_of(this) & $bit != 0)
        }
    };
}
predicate!(is_file, super::dirent::TYPE_FILE);
predicate!(is_directory, super::dirent::TYPE_DIR);
predicate!(is_symbolic_link, super::dirent::TYPE_SYMLINK);
predicate!(is_block_device, super::dirent::TYPE_BLOCK);
predicate!(is_character_device, super::dirent::TYPE_CHAR);
predicate!(is_fifo, super::dirent::TYPE_FIFO);
predicate!(is_socket, super::dirent::TYPE_SOCKET);

/// `fs.statSync(path)`. Raises a catchable error on failure (Node throws
/// `ENOENT`; this raises `entry::throw_type_error`, per rule 8 of
/// `rts-core-rwk`'s README — see that function's doc for why the class is
/// `TypeError` rather than Node's own). Follows a symlink at the final
/// component, matching Node.
pub(super) extern "C" fn stat_sync(_e: u64, _this: u64, path: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(path) = super::text(path) else {
        entry::throw_type_error("ENOENT: no such file or directory, stat");
        return entry::undefined_value();
    };
    match fetch(&path, true) {
        Ok(metadata) => build(&metadata),
        Err(_) => {
            entry::throw_type_error(&format!("ENOENT: no such file or directory, stat '{path}'"));
            entry::undefined_value()
        }
    }
}

/// `fs.lstatSync(path)` — identical to `statSync` except it does not follow a
/// symlink at the final path component (reports the link itself). Same raise
/// on failure.
pub(super) extern "C" fn lstat_sync(_e: u64, _this: u64, path: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(path) = super::text(path) else {
        entry::throw_type_error("ENOENT: no such file or directory, lstat");
        return entry::undefined_value();
    };
    match fetch(&path, false) {
        Ok(metadata) => build(&metadata),
        Err(_) => {
            entry::throw_type_error(&format!("ENOENT: no such file or directory, lstat '{path}'"));
            entry::undefined_value()
        }
    }
}

/// `follow`s a symlink at the final path component or not — one lookup, so
/// `stat_sync`/`lstat_sync` and [`promised`] (`fs.promises.stat`/`lstat`,
/// which must NOT throw — see that function's own doc) share the exact same
/// `std::fs` call rather than two copies drifting on which error a bad path
/// produces.
fn fetch(path: &str, follow: bool) -> std::io::Result<std::fs::Metadata> {
    match follow {
        true => std::fs::metadata(path),
        false => std::fs::symlink_metadata(path),
    }
}

/// `fs.promises.stat`/`fs.promises.lstat`'s own body, reusing [`fetch`] and
/// [`build`] rather than calling [`stat_sync`]/[`lstat_sync`] — those RAISE on
/// failure (`entry::throw_type_error`), and a promise wrapper cannot look at
/// that: `throw::in_flight`/`caught` are `pub(in crate::entry)`, not visible
/// outside `rts-core-rwk` (only `throw_type_error` itself is public), so a
/// caller in THIS crate has no way to ask whether the callee it just ran left
/// a throw behind — calling the throwing form and ignoring the answer would
/// be exactly the mistake rule 8 of that crate's README exists to name: the
/// throw stays in flight, unasked-about, and surfaces later as whatever
/// compiled code next happens to check. This never throws; it hands the
/// `io::Error` back for [`super::promises`] to turn into a rejection.
pub(super) fn stat_result(path: &str, follow: bool) -> std::io::Result<u64> {
    fetch(path, follow).map(|metadata| build(&metadata))
}

/// A `SystemTime` result as milliseconds since the epoch, `None` when the
/// platform/filesystem could not answer it (`Err`) or when it predates 1970
/// (`duration_since` fails the same way).
fn ms_of(result: std::io::Result<std::time::SystemTime>) -> Option<f64> {
    let time = result.ok()?;
    let elapsed = time.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(elapsed.as_secs_f64() * 1000.0)
}

#[cfg(unix)]
fn unix_numeric(metadata: &std::fs::Metadata) -> [(&'static str, f64); 9] {
    use std::os::unix::fs::MetadataExt;
    [
        ("dev", metadata.dev() as f64),
        ("ino", metadata.ino() as f64),
        ("mode", metadata.mode() as f64),
        ("nlink", metadata.nlink() as f64),
        ("uid", metadata.uid() as f64),
        ("gid", metadata.gid() as f64),
        ("rdev", metadata.rdev() as f64),
        ("blksize", metadata.blksize() as f64),
        ("blocks", metadata.blocks() as f64),
    ]
}

#[cfg(unix)]
fn ctime_ms(metadata: &std::fs::Metadata) -> Option<f64> {
    use std::os::unix::fs::MetadataExt;
    Some(metadata.ctime() as f64 * 1000.0 + metadata.ctime_nsec() as f64 / 1_000_000.0)
}

#[cfg(not(unix))]
fn ctime_ms(_metadata: &std::fs::Metadata) -> Option<f64> {
    None
}

/// The numbers Windows actually has, synthesized the way `libuv`'s
/// `uv__stat` (what real Node's `fs.statSync` runs through on Windows) does —
/// this used to answer `undefined` for every one of these on Windows, which
/// is what the module doc upstream of this function called "per-field
/// honesty": `0` is a legitimate device/inode number on some systems, so a
/// placeholder `0` would have read as an ANSWER. What was missing is that
/// Windows is not one of the systems with nothing to say here — `mode`,
/// `nlink`, `uid`, `gid`, `blksize` all have a real, checkable Windows value,
/// and reporting `undefined` for them made `chmodSync`'s own effect
/// unobservable (`statSync(f).mode` never changed) and failed `typeof uid ===
/// "number"` outright. `dev`/`ino`/`rdev` genuinely have no cheap answer from
/// `std::fs::Metadata` here (no volume serial number, no file index) and stay
/// `0` — the one number `libuv` itself reports for them on this platform too,
/// so `0` here is Node's own answer, not a stand-in invented for this crate.
#[cfg(windows)]
fn windows_numeric(metadata: &std::fs::Metadata) -> [(&'static str, f64); 9] {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_READONLY: u32 = 0x1;
    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
    let attributes = metadata.file_attributes();
    let mut mode: u32 = if attributes & FILE_ATTRIBUTE_DIRECTORY != 0 { 0o040000 } else { 0o100000 };
    mode |= if attributes & FILE_ATTRIBUTE_READONLY != 0 { 0o400 } else { 0o600 };
    // libuv duplicates the owner bits into group and other — Windows has no
    // separate group/other permission model, so it reports the same bits
    // three times rather than zeros a caller might read as "no access".
    mode |= (mode & 0o700) >> 3;
    mode |= (mode & 0o700) >> 6;
    let blocks = metadata.len().div_ceil(512);
    [
        ("dev", 0.0),
        ("ino", 0.0),
        ("mode", mode as f64),
        ("nlink", 1.0),
        ("uid", 0.0),
        ("gid", 0.0),
        ("rdev", 0.0),
        ("blksize", 4096.0),
        ("blocks", blocks as f64),
    ]
}

/// The `Stats`-shaped object every stat-family member answers.
pub(super) fn build(metadata: &std::fs::Metadata) -> u64 {
    let bits = type_bits(metadata.file_type());
    let size = metadata.len() as f64;
    let atime_ms = ms_of(metadata.accessed());
    let mtime_ms = ms_of(metadata.modified());
    let birthtime_ms = ms_of(metadata.created());
    let ctime_ms = ctime_ms(metadata);
    #[cfg(unix)]
    let numeric: &[(&str, f64)] = &unix_numeric(metadata);
    #[cfg(windows)]
    let numeric: &[(&str, f64)] = &windows_numeric(metadata);
    #[cfg(not(any(unix, windows)))]
    let numeric: &[(&str, f64)] = &[];
    const UNIX_ONLY: &[&str] = &["dev", "ino", "mode", "nlink", "uid", "gid", "rdev", "blksize", "blocks"];

    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "Stats", METHODS);
        let stats = entry::make_instance(context, prototype);
        let type_value = entry::make_number(bits as f64);
        entry::put_member(context, stats, "__type", type_value);
        let size_value = entry::make_number(size);
        entry::put_member(context, stats, "size", size_value);
        set_number(context, stats, "atimeMs", atime_ms);
        set_number(context, stats, "mtimeMs", mtime_ms);
        set_number(context, stats, "ctimeMs", ctime_ms);
        set_number(context, stats, "birthtimeMs", birthtime_ms);
        for &(name, value) in numeric {
            let field = entry::make_number(value);
            entry::put_member(context, stats, name, field);
        }
        #[cfg(not(any(unix, windows)))]
        for name in UNIX_ONLY {
            let undef = entry::undefined_in(context);
            entry::put_member(context, stats, name, undef);
        }
        #[cfg(any(unix, windows))]
        let _ = UNIX_ONLY;
        stats
    })
}

fn set_number(context: &mut Context, object: u64, name: &str, value: Option<f64>) {
    let field = match value {
        Some(value) => entry::make_number(value),
        None => entry::undefined_in(context),
    };
    entry::put_member(context, object, name, field);
}
