//! `fs.promises` — the promise-returning half of `node:fs`, against
//! `docs/reference/node/fs.md`'s promises section.
//!
//! # This is synchronous work wearing a `Promise`
//!
//! There is no asynchronous file I/O anywhere in this engine. Every member
//! below does the SAME work its `*Sync` sibling in [`super::basic`],
//! [`super::dirs`], [`super::stats`] or [`super::links`] does, before the
//! `Promise` it returns even exists, and hands that promise over already
//! settled via `rts_core::entry::settled` — see that function's own doc
//! comment, which this restates in the module's own words: the CALL blocks
//! where Node's does not, so two reads issued "concurrently" never actually
//! overlap — a program that starts several and awaits them all takes as long
//! as doing them one at a time. That is a real divergence, and it is a
//! divergence of TIMING, not of value: a caller gets the right bytes, and a
//! `.then` chained onto the result still runs on the microtask drain rather
//! than inline, because settling goes through the machine's queue like any
//! other settlement. A reader of this module must not come away believing
//! two `fs.promises` calls here run at once — they do not.
//!
//! # Reuse: every op is the `*Sync` body, called, not re-typed
//!
//! No member here re-implements a filesystem operation. Each calls straight
//! into the existing `*_sync` native from `basic`/`dirs`/`stats`/`links` and
//! wraps what comes back — that is what keeps a `readFile` here from ever
//! disagreeing with `readFileSync` about what a missing file answers.
//!
//! # Resolve vs. reject: where the `*Sync` return value has the signal, and
//! where it does not
//!
//! [`super`]'s own module doc states the `*Sync` convention: a member that
//! ANSWERS something (`readFileSync`, `readdirSync`, `statSync`, `lstatSync`,
//! `readlinkSync`, `realpathSync`, `mkdtempSync`) answers `undefined` on
//! failure and never on success (an empty file/dir is `""`/`[]`, not
//! `undefined`). For those, this module has a real signal to reuse: passing
//! that same `undefined` on to `settled(..., rejected: true)` turns the
//! stated "answers undefined on failure" convention into "rejects", which is
//! the one place reuse needs care rather than a straight call — resolving a
//! `fs.promises.readFile` with `undefined` for a missing file would be a
//! silent wrong answer a caller's `.catch` could never see.
//!
//! A member whose `*Sync` form has NO other value to give
//! (`writeFileSync`, `appendFileSync`, `mkdirSync`, `rmSync`, `rmdirSync`,
//! `copyFileSync`, `renameSync`, `unlinkSync`, `symlinkSync`, `linkSync`,
//! `chmodSync`, `truncateSync`, `cpSync`, `accessSync`) answers `undefined`
//! on success AND on failure alike — that is `super`'s own stated gap, not
//! one introduced here. Reusing that return value gives no signal to reject
//! on, and this module was told not to touch `basic.rs`/`dirs.rs`/`links.rs`
//! to add one (their `Result` is discarded with `let _ =` before this module
//! ever sees a return value) — inventing a signal here would mean either a
//! second `std::fs` call this module owns nothing to justify, or guessing
//! success from a side effect (e.g. "does the path exist now") that is wrong
//! for concurrent writers and for `rm`/`unlink`, which succeed by making the
//! path NOT exist. So these resolve with `undefined` unconditionally,
//! matching what the `*Sync` form already told a caller: nothing here can
//! say which of success/failure happened. Named as the hole in the report
//! for this task, not papered over.

//! # What this task added, and what it did not
//!
//! `chown`/`lchown`/`fchown`/`fchmod`/`lchmod`/`utimes`/`lutimes`/`futimes`
//! (over [`super::perms`]) and `statfs` (over [`super::statfs`]) follow the
//! exact reuse rule above: each calls its `*Sync` sibling and reads
//! [`super::succeeded`]/the sync return value for the reject signal, same as
//! every member already here.
//!
//! `fs.promises.glob` is NOT implemented: real Node answers an
//! `AsyncIterable`, not a resolved array — a fundamentally different shape
//! from every other member of this module, which settles one `Promise` and
//! is done. Building an async-iterable here needs the same missing
//! "construct a promise from Rust and drive it" capability
//! `crate::events`' own module doc names for `events.on`/`events.once`.
//! `fs.globSync` (see [`super::glob`]) is implemented and is what a caller
//! wanting a resolved array should call, awaiting nothing.
//!
//! `fs.promises.watch` is NOT implemented for the same reason — also an
//! `AsyncIterable`, and additionally reads to a listener the queueing model
//! [`super::watch`]'s module doc describes has nowhere synchronous to
//! deliver into. `fs.watch`/`fs.watchFile` (callback/`EventEmitter` forms)
//! are implemented; see that module for exactly when a listener fires.

use rts_core::entry::{settled, undefined_in, Context, Provided};

use super::{basic, dirs, handle, links, perms, statfs, stats};

/// The namespace `fs.promises` is. Not registered as `node:fs/promises` here
/// — that specifier is the coordinator's line in `lib.rs`.
pub(crate) fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("open", handle::open),
        ("readFile", read_file),
        ("writeFile", write_file),
        ("appendFile", append_file),
        ("mkdir", mkdir),
        ("rm", rm),
        ("readdir", readdir),
        ("stat", stat),
        ("lstat", lstat),
        ("access", access),
        ("copyFile", copy_file),
        ("rename", rename),
        ("unlink", unlink),
        ("rmdir", rmdir),
        ("readlink", readlink),
        ("realpath", realpath),
        ("symlink", symlink),
        ("link", link),
        ("chmod", chmod),
        ("truncate", truncate),
        ("mkdtemp", mkdtemp),
        ("cp", cp),
        ("chown", chown),
        ("lchown", lchown),
        ("fchown", fchown),
        ("fchmod", fchmod),
        ("lchmod", lchmod),
        ("utimes", utimes),
        ("lutimes", lutimes),
        ("futimes", futimes),
        ("statfs", statfs_promise),
    ];
    rts_core::entry::make_namespace(context, members)
}

/// Wraps a `*Sync` native whose `undefined` return means FAILURE — rejects
/// with a real error object on `undefined`, resolves with the value
/// otherwise.
///
/// # Never rejects with the bare `undefined`
///
/// The `*Sync` failure signal IS `undefined`, and passing it straight to
/// `settled` used to be exactly that — a promise "rejected" with a value
/// indistinguishable, at the `await`/`catch` boundary, from no rejection
/// value at all. See [`super::node_error`]'s own doc for the failure that
/// produced and why a constructed object is what closes it: every member
/// wrapped here now rejects with `{ code, message }`, `code` read from
/// [`super::last_code`] — set by the SAME `*_sync` call this wraps, via
/// [`super::record_io`], so it is never stale.
fn answered(sync_result: u64) -> u64 {
    rts_core::entry::with_runtime(|context| {
        let absent = undefined_in(context);
        match sync_result == absent {
            true => {
                let code = super::last_code();
                let error = super::node_error(context, code, &format!("{code}: operation failed"));
                settled(context, error, true)
            }
            false => settled(context, sync_result, false),
        }
    })
}

/// Wraps a `*Sync` native that answers `undefined` whether it worked or not.
///
/// It rejects when the operation failed, which it learns from
/// [`super::succeeded`] — read IMMEDIATELY, with nothing between the sync call
/// and this, because a second operation overwrites it. That is `errno`'s rule
/// and `super`'s documentation says why it is `errno`'s design.
///
/// This always RESOLVED for one commit, and the consequence is the reason the
/// channel exists: a `.then` success path ran after a write that never
/// happened. Rejects with a real object, same as [`answered`] — not the bare
/// `sync_result`, which is `undefined` on both success and failure for every
/// member this wraps and so carries no signal a `catch` could read anyway.
fn done(sync_result: u64) -> u64 {
    let failed = !super::succeeded();
    rts_core::entry::with_runtime(|context| match failed {
        true => {
            let code = super::last_code();
            let error = super::node_error(context, code, &format!("{code}: operation failed"));
            settled(context, error, true)
        }
        false => settled(context, sync_result, false),
    })
}

pub(super) extern "C" fn read_file(e: u64, this: u64, path: u64, encoding: u64, a2: u64, a3: u64) -> u64 {
    answered(basic::read_file_sync(e, this, path, encoding, a2, a3))
}

pub(super) extern "C" fn write_file(e: u64, this: u64, path: u64, data: u64, a2: u64, a3: u64) -> u64 {
    done(basic::write_file_sync(e, this, path, data, a2, a3))
}

pub(super) extern "C" fn append_file(e: u64, this: u64, path: u64, data: u64, a2: u64, a3: u64) -> u64 {
    done(basic::append_file_sync(e, this, path, data, a2, a3))
}

pub(super) extern "C" fn copy_file(e: u64, this: u64, src: u64, dest: u64, a2: u64, a3: u64) -> u64 {
    done(basic::copy_file_sync(e, this, src, dest, a2, a3))
}

pub(super) extern "C" fn rename(e: u64, this: u64, from: u64, to: u64, a2: u64, a3: u64) -> u64 {
    done(basic::rename_sync(e, this, from, to, a2, a3))
}

pub(super) extern "C" fn unlink(e: u64, this: u64, path: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    done(basic::unlink_sync(e, this, path, a1, a2, a3))
}

pub(super) extern "C" fn mkdir(e: u64, this: u64, path: u64, options: u64, a2: u64, a3: u64) -> u64 {
    done(dirs::mkdir_sync(e, this, path, options, a2, a3))
}

pub(super) extern "C" fn rm(e: u64, this: u64, path: u64, options: u64, a2: u64, a3: u64) -> u64 {
    done(dirs::rm_sync(e, this, path, options, a2, a3))
}

pub(super) extern "C" fn rmdir(e: u64, this: u64, path: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    done(dirs::rmdir_sync(e, this, path, a1, a2, a3))
}

pub(super) extern "C" fn readdir(e: u64, this: u64, path: u64, options: u64, a2: u64, a3: u64) -> u64 {
    answered(dirs::readdir_sync(e, this, path, options, a2, a3))
}

pub(super) extern "C" fn mkdtemp(e: u64, this: u64, prefix: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    answered(dirs::mkdtemp_sync(e, this, prefix, a1, a2, a3))
}

pub(super) extern "C" fn cp(e: u64, this: u64, src: u64, dest: u64, options: u64, a3: u64) -> u64 {
    done(dirs::cp_sync(e, this, src, dest, options, a3))
}

/// Does NOT call [`stats::stat_sync`] — see [`stats::stat_result`]'s own doc
/// for why that would leave a throw in flight for nobody to ask about.
pub(super) extern "C" fn stat(_e: u64, _this: u64, path: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    settle_stat(path, true)
}

/// See [`stat`].
pub(super) extern "C" fn lstat(_e: u64, _this: u64, path: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    settle_stat(path, false)
}

fn settle_stat(path: u64, follow: bool) -> u64 {
    let path_text = super::text(path);
    // `stat_result` (through `build`) opens its OWN `with_runtime` — computed
    // here, before the outer one below opens, rather than nested inside it:
    // `with_current` holds a borrow for as long as its body runs, and a
    // second `with_runtime` while the first is still open panics on the
    // re-entry (the same trap `fs::watch`'s module doc names for
    // `set_prototype`/`set_prototype_in`).
    let outcome = path_text.as_deref().map(|path_text| (path_text.to_owned(), stats::stat_result(path_text, follow)));
    rts_core::entry::with_runtime(|context| match outcome {
        Some((_, Ok(value))) => settled(context, value, false),
        Some((path_text, Err(io_error))) => {
            let code = super::io_node_code(io_error.kind());
            let error = super::node_error(context, code, &format!("{code}: no such file or directory, stat '{path_text}'"));
            settled(context, error, true)
        }
        None => {
            let error = super::node_error(context, "ENOENT", "ENOENT: no such file or directory, stat");
            settled(context, error, true)
        }
    })
}

pub(super) extern "C" fn access(e: u64, this: u64, path: u64, mode: u64, a2: u64, a3: u64) -> u64 {
    done(links::access_sync(e, this, path, mode, a2, a3))
}

pub(super) extern "C" fn readlink(e: u64, this: u64, path: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    answered(links::readlink_sync(e, this, path, a1, a2, a3))
}

pub(super) extern "C" fn realpath(e: u64, this: u64, path: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    answered(links::realpath_sync(e, this, path, a1, a2, a3))
}

pub(super) extern "C" fn symlink(e: u64, this: u64, target: u64, path: u64, kind: u64, a3: u64) -> u64 {
    done(links::symlink_sync(e, this, target, path, kind, a3))
}

pub(super) extern "C" fn link(e: u64, this: u64, existing: u64, new: u64, a2: u64, a3: u64) -> u64 {
    done(links::link_sync(e, this, existing, new, a2, a3))
}

pub(super) extern "C" fn chmod(e: u64, this: u64, path: u64, mode: u64, a2: u64, a3: u64) -> u64 {
    done(links::chmod_sync(e, this, path, mode, a2, a3))
}

pub(super) extern "C" fn truncate(e: u64, this: u64, path: u64, len: u64, a2: u64, a3: u64) -> u64 {
    done(links::truncate_sync(e, this, path, len, a2, a3))
}

pub(super) extern "C" fn chown(e: u64, this: u64, path: u64, uid: u64, gid: u64, a3: u64) -> u64 {
    done(perms::chown_sync(e, this, path, uid, gid, a3))
}

pub(super) extern "C" fn lchown(e: u64, this: u64, path: u64, uid: u64, gid: u64, a3: u64) -> u64 {
    done(perms::lchown_sync(e, this, path, uid, gid, a3))
}

pub(super) extern "C" fn fchown(e: u64, this: u64, fd: u64, uid: u64, gid: u64, a3: u64) -> u64 {
    done(perms::fchown_sync(e, this, fd, uid, gid, a3))
}

pub(super) extern "C" fn fchmod(e: u64, this: u64, fd: u64, mode: u64, a2: u64, a3: u64) -> u64 {
    done(perms::fchmod_sync(e, this, fd, mode, a2, a3))
}

pub(super) extern "C" fn lchmod(e: u64, this: u64, path: u64, mode: u64, a2: u64, a3: u64) -> u64 {
    done(perms::lchmod_sync(e, this, path, mode, a2, a3))
}

pub(super) extern "C" fn utimes(e: u64, this: u64, path: u64, atime: u64, mtime: u64, a3: u64) -> u64 {
    done(perms::utimes_sync(e, this, path, atime, mtime, a3))
}

pub(super) extern "C" fn lutimes(e: u64, this: u64, path: u64, atime: u64, mtime: u64, a3: u64) -> u64 {
    done(perms::lutimes_sync(e, this, path, atime, mtime, a3))
}

pub(super) extern "C" fn futimes(e: u64, this: u64, fd: u64, atime: u64, mtime: u64, a3: u64) -> u64 {
    done(perms::futimes_sync(e, this, fd, atime, mtime, a3))
}

/// `fsPromises.statfs(path, options?)` — [`answered`], reusing `statfsSync`'s
/// own "`undefined` means failure" convention (a real answer is always a
/// `StatFs` object).
pub(super) extern "C" fn statfs_promise(e: u64, this: u64, path: u64, options: u64, a2: u64, a3: u64) -> u64 {
    answered(statfs::statfs_sync(e, this, path, options, a2, a3))
}
