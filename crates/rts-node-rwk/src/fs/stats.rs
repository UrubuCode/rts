//! `statSync`/`lstatSync` — the `fs.Stats`-shaped objects built over
//! `std::fs::Metadata`.
//!
//! `fstatSync` (the fd-based form) lives in [`super::fd`] beside the fd table
//! it needs, and calls [`build`] from here rather than duplicating it.

use super::text;

/// `fs.statSync(path)` — an object with `size`, `isFile()`, `isDirectory()`,
/// `isSymbolicLink()`. Follows a symlink at the final component, matching
/// Node.
///
/// `undefined` on failure.
pub(super) extern "C" fn stat_sync(_e: u64, _this: u64, path: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(path) = text(path) else {
        return rts_core_rwk::entry::undefined_value();
    };
    let Ok(metadata) = std::fs::metadata(&path) else {
        return rts_core_rwk::entry::undefined_value();
    };
    build(&metadata, false)
}

/// `fs.lstatSync(path)` — identical to `statSync` except it does not follow
/// a symlink at the final path component (reports the link itself).
pub(super) extern "C" fn lstat_sync(_e: u64, _this: u64, path: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(path) = text(path) else {
        return rts_core_rwk::entry::undefined_value();
    };
    let Ok(metadata) = std::fs::symlink_metadata(&path) else {
        return rts_core_rwk::entry::undefined_value();
    };
    let is_symlink = metadata.is_symlink();
    build(&metadata, is_symlink)
}

/// The `Stats`-shaped object every stat-family member answers.
///
/// `size` is a NUMBER, which it was not for the length of one review: the
/// host-facing surface had no numeric constructor, so the first version of
/// this answered a decimal string and said so rather than smuggling a byte
/// count through as some other type. `make_number` exists because of that
/// report — the gap was in the API this module was given, not in the module.
///
/// `dev`/`ino`/`mode`/`nlink`/`uid`/`gid`/`rdev`/`blksize`/`blocks` and every
/// `*Ms`/`*Ns`/`Date` time field from the reference doc are NOT built: they
/// need `std::os::unix::fs::MetadataExt`/`std::os::windows::fs::MetadataExt`
/// behind a `cfg` this module did not add given everything else still on the
/// "not implemented" list, and a `Date` value this value API has no
/// constructor for. `size`/`isFile`/`isDirectory`/`isSymbolicLink` are the
/// subset every earlier version of this module already answered.
pub(super) fn build(metadata: &std::fs::Metadata, is_symlink: bool) -> u64 {
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
        let is_symlink_fn = rts_core_rwk::entry::make_callable(context, is_symlink_answer(is_symlink));
        rts_core_rwk::entry::put_member(context, stats, "isSymbolicLink", is_symlink_fn);
        stats
    })
}

/// Picks the zero-argument native that answers a fixed `isFile()` result.
///
/// Two functions rather than one closing over a captured bool: a
/// `Provided`/`Native` here is a bare `extern "C" fn`, with no room for
/// captured state, so the two possible answers are two statically distinct
/// functions instead.
fn is_file_answer(value: bool) -> rts_core_rwk::entry::Provided {
    match value {
        true => is_file_true,
        false => is_file_false,
    }
}

/// Picks the zero-argument native that answers a fixed `isDirectory()` result.
fn is_dir_answer(value: bool) -> rts_core_rwk::entry::Provided {
    match value {
        true => is_dir_true,
        false => is_dir_false,
    }
}

/// Picks the zero-argument native that answers a fixed `isSymbolicLink()` result.
fn is_symlink_answer(value: bool) -> rts_core_rwk::entry::Provided {
    match value {
        true => is_symlink_true,
        false => is_symlink_false,
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
extern "C" fn is_symlink_true(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    rts_core_rwk::entry::boolean_value(true)
}
extern "C" fn is_symlink_false(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    rts_core_rwk::entry::boolean_value(false)
}
