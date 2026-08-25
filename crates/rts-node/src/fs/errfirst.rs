//! The err-first callback forms of the members whose `*Sync` sibling already
//! answers everything they need — `rename`, `unlink`, `chown`, `close`,
//! `mkdtemp` and the rest of that column.
//!
//! # Why these exist now, and what they are NOT
//!
//! They are not background I/O. Every member here does exactly what
//! [`super::callbacks`] does and for the reason stated there: the work runs to
//! completion inside the native call and the callback is invoked once, before
//! it returns. What is new is only the COVERAGE — `fs.chown(path, …)` was not
//! a function at all, so a program calling it got `undefined is not a
//! function` where Node raises `ERR_INVALID_ARG_TYPE`, and no argument check
//! anywhere could have answered that.
//!
//! # The validation is the sync member's own, asked about rather than repeated
//!
//! Each member below CALLS its `*Sync` sibling, which validates its arguments
//! through [`super::validate`] and registers a throw when it refuses. The
//! wrapper then asks [`rts_core::entry::thrown`] whether one is pending and
//! returns without invoking the callback — which is Node's own behaviour
//! (`assert.throws(() => fs.chown(1, 1, 1, common.mustNotCall()))` asserts
//! both halves: it throws, AND the callback never runs).
//!
//! Writing the checks again here would be the second copy of a rule this
//! crate's README forbids, and would drift on the first argument whose
//! accepted set changes.
//!
//! # The outcome channel, and the one thing it cannot tell us
//!
//! `succeeded`/`last_code` are the `errno`-shaped side channel the parent
//! module documents. It is set to "succeeded" before each body runs, because a
//! `*Sync` member that fails without recording — `truncateSync` when the open
//! itself fails — would otherwise report the PREVIOUS call's outcome, which is
//! the one answer worse than "no error". A failure this crate cannot see is
//! therefore reported as success, which is the same thing the `*Sync` form
//! already answers to a caller: `undefined`, either way.

use rts_core::entry;

use super::validate;

/// Invokes `callback(err)` / `callback(err, value)`.
fn invoke(callback: u64, err: u64, value: u64) {
    let absent = entry::undefined_value();
    entry::call(callback, absent, err, value, absent, absent);
}

/// A Node-shaped error object for the last failure, built from the outcome
/// channel. `verb` is the syscall name Node puts in the message.
fn failure(verb: &str, code: &str) -> u64 {
    let message = format!("{code}: {verb} failed");
    entry::with_runtime(|context| super::node_error(context, code, &message))
}

/// Runs an err-only `*Sync` body and settles its callback.
///
/// `body` is the sibling itself, called with the four value slots already
/// resolved by the member — the shift below is the member's business, not
/// this one's.
fn settle(callback: u64, verb: &str, body: impl FnOnce()) -> u64 {
    let absent = entry::undefined_value();
    // The callback is checked FIRST: a member that ran its work and then found
    // it had nowhere to report to would have done the side effect Node refuses
    // before the argument was ever accepted.
    if !validate::callback(callback) {
        return absent;
    }
    super::record(true);
    body();
    if entry::thrown() != 0 {
        // A refusal the sibling raised. The callback must NOT run: every
        // `*-type-check.js` file in Node's suite passes `common.mustNotCall()`
        // precisely to pin that.
        return absent;
    }
    match super::succeeded() {
        true => invoke(callback, entry::null_value(), absent),
        false => invoke(callback, failure(verb, super::last_code()), absent),
    }
    absent
}

/// Runs a value-answering `*Sync` body and settles its callback with what it
/// answered — `undefined` being the failure the `*Sync` forms document.
fn settle_value(callback: u64, verb: &str, body: impl FnOnce() -> u64) -> u64 {
    let absent = entry::undefined_value();
    if !validate::callback(callback) {
        return absent;
    }
    super::record(true);
    let value = body();
    if entry::thrown() != 0 {
        return absent;
    }
    match value == absent {
        // `ENOENT` and not `last_code`: none of the value-answering siblings
        // records an outcome (they answer `undefined` instead), so the code
        // here is the one a missing path gives — named rather than guessed at
        // more precisely than the sibling actually knows.
        true => invoke(callback, failure(verb, "ENOENT"), absent),
        false => invoke(callback, entry::null_value(), value),
    }
    absent
}

/// An optional argument and the callback after it, told apart.
///
/// `fs.mkdir(path, cb)` puts the function in the OPTIONS slot; `fs.mkdir(path,
/// opts, cb)` does not. Answers `(option, callback)`.
///
/// The test is "is the later slot filled", NOT "is the earlier one callable":
/// `fs.truncate(path, {})` must refuse `{}` as a callback rather than read it
/// as an option and then find no callback at all — which is the refusal
/// `test-fs-make-callback.js` asserts.
fn shifted(option: u64, callback: u64) -> (u64, u64) {
    let absent = entry::undefined_value();
    match callback == absent {
        true => (absent, option),
        false => (option, callback),
    }
}

/// `fs.rename(oldPath, newPath, cb)`.
pub(super) extern "C" fn rename(e: u64, this: u64, old: u64, new: u64, callback: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    settle(callback, "rename", || {
        super::basic::rename_sync(e, this, old, new, absent, absent);
    })
}

/// `fs.unlink(path, cb)`.
pub(super) extern "C" fn unlink(e: u64, this: u64, path: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    settle(callback, "unlink", || {
        super::basic::unlink_sync(e, this, path, absent, absent, absent);
    })
}

/// `fs.copyFile(src, dest[, mode], cb)` — `mode` is accepted and dropped, the
/// same way `copyFileSync` drops it.
pub(super) extern "C" fn copy_file(e: u64, this: u64, src: u64, dest: u64, mode: u64, callback: u64) -> u64 {
    let absent = entry::undefined_value();
    let (_mode, callback) = shifted(mode, callback);
    settle(callback, "copyfile", || {
        super::basic::copy_file_sync(e, this, src, dest, absent, absent);
    })
}

/// `fs.appendFile(path, data[, options], cb)`.
pub(super) extern "C" fn append_file(e: u64, this: u64, path: u64, data: u64, options: u64, callback: u64) -> u64 {
    let absent = entry::undefined_value();
    let (_options, callback) = shifted(options, callback);
    settle(callback, "open", || {
        super::basic::append_file_sync(e, this, path, data, absent, absent);
    })
}

/// `fs.mkdir(path[, options], cb)`.
pub(super) extern "C" fn mkdir(e: u64, this: u64, path: u64, options: u64, callback: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let (options, callback) = shifted(options, callback);
    settle(callback, "mkdir", || {
        super::dirs::mkdir_sync(e, this, path, options, absent, absent);
    })
}

/// `fs.rmdir(path[, options], cb)`.
pub(super) extern "C" fn rmdir(e: u64, this: u64, path: u64, options: u64, callback: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let (_options, callback) = shifted(options, callback);
    settle(callback, "rmdir", || {
        super::dirs::rmdir_sync(e, this, path, absent, absent, absent);
    })
}

/// `fs.rm(path[, options], cb)`.
pub(super) extern "C" fn rm(e: u64, this: u64, path: u64, options: u64, callback: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let (options, callback) = shifted(options, callback);
    settle(callback, "rm", || {
        super::dirs::rm_sync(e, this, path, options, absent, absent);
    })
}

/// `fs.chmod(path, mode, cb)`.
pub(super) extern "C" fn chmod(e: u64, this: u64, path: u64, mode: u64, callback: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    settle(callback, "chmod", || {
        super::links::chmod_sync(e, this, path, mode, absent, absent);
    })
}

/// `fs.link(existingPath, newPath, cb)`.
pub(super) extern "C" fn link(e: u64, this: u64, existing: u64, new: u64, callback: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    settle(callback, "link", || {
        super::links::link_sync(e, this, existing, new, absent, absent);
    })
}

/// `fs.symlink(target, path[, type], cb)`.
pub(super) extern "C" fn symlink(e: u64, this: u64, target: u64, path: u64, kind: u64, callback: u64) -> u64 {
    let absent = entry::undefined_value();
    let (kind, callback) = shifted(kind, callback);
    settle(callback, "symlink", || {
        super::links::symlink_sync(e, this, target, path, kind, absent);
    })
}

/// `fs.truncate(path[, len], cb)`.
pub(super) extern "C" fn truncate(e: u64, this: u64, path: u64, len: u64, callback: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let (len, callback) = shifted(len, callback);
    settle(callback, "ftruncate", || {
        super::links::truncate_sync(e, this, path, len, absent, absent);
    })
}

/// `fs.ftruncate(fd[, len], cb)`.
pub(super) extern "C" fn ftruncate(e: u64, this: u64, fd: u64, len: u64, callback: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let (len, callback) = shifted(len, callback);
    settle(callback, "ftruncate", || {
        super::fd::ftruncate_sync(e, this, fd, len, absent, absent);
    })
}

/// `fs.close(fd, cb)` — `cb` is OPTIONAL in Node here, but a non-function that
/// is present is still refused, which is what `test-fs-close-errors.js` pins.
pub(super) extern "C" fn close(e: u64, this: u64, fd: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    if callback == absent {
        super::fd::close_sync(e, this, fd, absent, absent, absent);
        return absent;
    }
    settle(callback, "close", || {
        super::fd::close_sync(e, this, fd, absent, absent, absent);
    })
}

/// `fs.chown(path, uid, gid, cb)`.
pub(super) extern "C" fn chown(e: u64, this: u64, path: u64, uid: u64, gid: u64, callback: u64) -> u64 {
    let absent = entry::undefined_value();
    settle(callback, "chown", || {
        super::perms::chown_sync(e, this, path, uid, gid, absent);
    })
}

/// `fs.lchown(path, uid, gid, cb)`.
pub(super) extern "C" fn lchown(e: u64, this: u64, path: u64, uid: u64, gid: u64, callback: u64) -> u64 {
    let absent = entry::undefined_value();
    settle(callback, "lchown", || {
        super::perms::lchown_sync(e, this, path, uid, gid, absent);
    })
}

/// `fs.fchown(fd, uid, gid, cb)`.
pub(super) extern "C" fn fchown(e: u64, this: u64, fd: u64, uid: u64, gid: u64, callback: u64) -> u64 {
    let absent = entry::undefined_value();
    settle(callback, "fchown", || {
        super::perms::fchown_sync(e, this, fd, uid, gid, absent);
    })
}

/// `fs.fchmod(fd, mode, cb)`.
pub(super) extern "C" fn fchmod(e: u64, this: u64, fd: u64, mode: u64, callback: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    settle(callback, "fchmod", || {
        super::perms::fchmod_sync(e, this, fd, mode, absent, absent);
    })
}

/// `fs.lchmod(path, mode, cb)`.
pub(super) extern "C" fn lchmod(e: u64, this: u64, path: u64, mode: u64, callback: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    settle(callback, "lchmod", || {
        super::perms::lchmod_sync(e, this, path, mode, absent, absent);
    })
}

/// `fs.readlink(path, cb)`.
pub(super) extern "C" fn readlink(e: u64, this: u64, path: u64, options: u64, callback: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let (_options, callback) = shifted(options, callback);
    settle_value(callback, "readlink", || super::links::readlink_sync(e, this, path, absent, absent, absent))
}

/// `fs.realpath(path, cb)`.
pub(super) extern "C" fn realpath(e: u64, this: u64, path: u64, options: u64, callback: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let (_options, callback) = shifted(options, callback);
    settle_value(callback, "realpath", || super::links::realpath_sync(e, this, path, absent, absent, absent))
}

/// `fs.mkdtemp(prefix[, options], cb)`.
pub(super) extern "C" fn mkdtemp(e: u64, this: u64, prefix: u64, options: u64, callback: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let (_options, callback) = shifted(options, callback);
    settle_value(callback, "mkdtemp", || super::dirs::mkdtemp_sync(e, this, prefix, absent, absent, absent))
}

/// `fs.open(path[, flags[, mode]], cb)`.
///
/// Two optional slots, so the shift is applied twice — `mode` is dropped
/// either way, matching `openSync`'s own stated limit.
pub(super) extern "C" fn open(e: u64, this: u64, path: u64, flags: u64, mode: u64, callback: u64) -> u64 {
    let absent = entry::undefined_value();
    let (_mode, callback) = shifted(mode, callback);
    let (flags, callback) = shifted(flags, callback);
    settle_value(callback, "open", || super::fd::open_sync(e, this, path, flags, absent, absent))
}
