//! `Dirent` — one shared prototype, over `docs/reference/node/fs.md`'s
//! `Dirent` section.
//!
//! # Why a bitmask and not per-instance closures
//!
//! `stats.rs`'s old `Stats` and this class's own predecessor both hung a
//! fixed-answer closure directly on each instance — two statically distinct
//! `extern "C" fn`s per boolean, because a [`Provided`] has no captured-state
//! slot. That gave every instance its OWN methods, which is wrong the same way
//! `events.rs`'s module doc states for `EventEmitter`: `Object.keys` picked up
//! the methods, and two `Dirent`s shared no prototype.
//!
//! [`entry::make_prototype`] fixes the sharing; the closures still cannot
//! capture anything, so the seven `is*` predicates are all ONE function each
//! (not seven-times-two), reading a hidden `__type` bitmask off `this` instead
//! of off a captured bool. `stats.rs`'s `Stats` prototype reuses this exact
//! tagging (see [`type_bits`]) rather than inventing its own, so a `Dirent`
//! and a `Stats` built from the same `std::fs::FileType` never disagree about
//! what it was.

use rts_core_rwk::entry::{self, Context, Provided};

pub(super) const TYPE_FILE: u32 = 1;
pub(super) const TYPE_DIR: u32 = 2;
pub(super) const TYPE_SYMLINK: u32 = 4;
pub(super) const TYPE_BLOCK: u32 = 8;
pub(super) const TYPE_CHAR: u32 = 16;
pub(super) const TYPE_FIFO: u32 = 32;
pub(super) const TYPE_SOCKET: u32 = 64;

const METHODS: &[(&str, Provided)] = &[
    ("isFile", is_file),
    ("isDirectory", is_directory),
    ("isSymbolicLink", is_symbolic_link),
    ("isBlockDevice", is_block_device),
    ("isCharacterDevice", is_character_device),
    ("isFIFO", is_fifo),
    ("isSocket", is_socket),
];

/// The bitmask `this.__type` holds, `0` for a `this` that is not one of these
/// (or carries no such property at all — `number_of` on `undefined` answers
/// `None`, which folds to no bits set).
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
predicate!(is_file, TYPE_FILE);
predicate!(is_directory, TYPE_DIR);
predicate!(is_symbolic_link, TYPE_SYMLINK);
predicate!(is_block_device, TYPE_BLOCK);
predicate!(is_character_device, TYPE_CHAR);
predicate!(is_fifo, TYPE_FIFO);
predicate!(is_socket, TYPE_SOCKET);

/// The bit tag a `std::fs::FileType` carries.
///
/// Block/char/FIFO/socket only answer on Unix — `std::fs::FileType` has no
/// cross-platform accessor for them (`FileTypeExt` is a Unix-only trait), and
/// Windows has no such distinction at the `std` layer to report honestly, so
/// those four bits are simply never set there rather than guessed at.
pub(super) fn type_bits(file_type: std::fs::FileType) -> u32 {
    let mut bits = 0u32;
    if file_type.is_file() {
        bits |= TYPE_FILE;
    }
    if file_type.is_dir() {
        bits |= TYPE_DIR;
    }
    if file_type.is_symlink() {
        bits |= TYPE_SYMLINK;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        if file_type.is_block_device() {
            bits |= TYPE_BLOCK;
        }
        if file_type.is_char_device() {
            bits |= TYPE_CHAR;
        }
        if file_type.is_fifo() {
            bits |= TYPE_FIFO;
        }
        if file_type.is_socket() {
            bits |= TYPE_SOCKET;
        }
    }
    bits
}

/// A `Dirent` instance for one directory entry.
///
/// `parentPath` and the deprecated `path` alias (Node kept both; `path` is
/// `parentPath` under a name being phased out) are the same value — an
/// instance carries the directory it came out of, not the entry's own full
/// path, matching Node.
pub(super) fn build(context: &mut Context, name: &str, parent_path: &str, bits: u32) -> u64 {
    let prototype = entry::make_prototype(context, "Dirent", METHODS);
    let instance = entry::make_instance(context, prototype);
    let type_value = entry::make_number(bits as f64);
    entry::put_member(context, instance, "__type", type_value);
    let name_value = entry::make_string(context, name);
    entry::put_member(context, instance, "name", name_value);
    let parent_value = entry::make_string(context, parent_path);
    entry::put_member(context, instance, "parentPath", parent_value);
    entry::put_member(context, instance, "path", parent_value);
    instance
}
