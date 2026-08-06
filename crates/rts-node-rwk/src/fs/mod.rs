//! `node:fs` — the SYNCHRONOUS surface, over `std::fs`, against
//! `docs/reference/node/fs.md`.
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
//! there is no Buffer-like object standing in for it. The same gap is why
//! `readSync`/`writeSync`/`readvSync`/`writevSync` (which take a
//! `Uint8Array`/`Buffer` argument, not a path) are refused below rather than
//! given a wrong shape.
//!
//! # Errors: no throw, a stated stand-in
//!
//! Node throws on a missing file, a permission failure, and so on. A native
//! entry point here cannot throw across the call boundary, so each member
//! states its OWN stand-in rather than sharing one:
//! - a read that answers a value (`readFileSync`, `readdirSync`,
//!   `readlinkSync`, `realpathSync`, `statSync`, `lstatSync`, `fstatSync`,
//!   `mkdtempSync`) answers `undefined` on failure — never `""` or `[]`,
//!   which would be a wrong answer that runs silently.
//! - an operation whose real answer is boolean (`existsSync`) answers `false`.
//! - an operation with no other value to give (`writeFileSync`,
//!   `appendFileSync`, `mkdirSync`, `rmSync`, `copyFileSync`, `renameSync`,
//!   `unlinkSync`, `rmdirSync`, `symlinkSync`, `linkSync`, `chmodSync`,
//!   `truncateSync`, `cpSync`, `accessSync`, `closeSync`, `fsyncSync`,
//!   `fdatasyncSync`, `ftruncateSync`) answers `undefined` on success AND on
//!   failure alike; a caller that needs to know which happened has nothing
//!   here to ask, which is the gap to name rather than paper over.
//! - `openSync` answers `undefined` (rather than a negative "errno" number)
//!   on failure — a negative fd is a plausible-looking wrong value the same
//!   way `""` would be for a read.
//!
//! # `openSync`'s fd: this module's own table, not the OS's
//!
//! A fd handed to `readSync`/`writeSync` in real Node is the OS's own integer
//! and can be used by any other process/tool. This module's `openSync` opens
//! a real `std::fs::File` but keys it in a private table
//! ([`fd::TABLE`]) rather than returning `File::as_raw_fd()` — the numbers
//! are this module's own and only meaningful to `closeSync`/`fstatSync`/
//! `fsyncSync`/`fdatasyncSync`/`ftruncateSync`, which is every fd-based
//! member this module can give a real answer to without a `Buffer` to read
//! or write into.
//!
//! # Not implemented, by name
//!
//! - **Every async/Promise/callback form** — `readFile`, `writeFile`,
//!   `fs.promises.*`, `fsPromises.*`, every bare (non-`Sync`) top-level
//!   function, `FileHandle` and all its methods. Blocked on the same
//!   no-user-code-reentry limit as the module doc states; out of scope per
//!   this task's own instructions.
//! - **`readSync`/`writeSync`/`readvSync`/`writevSync`** — need a
//!   `Buffer`/`Uint8Array`/`TypedArray` argument. The value API this module
//!   was given (`rts_core_rwk::entry`) has no accessor for typed-array
//!   backing bytes at all — not a missing wrapper here, a missing primitive
//!   in the API. Named as the hole in the report for this task.
//! - **`watch`, `watchFile`, `unwatchFile`, `FSWatcher`, `StatWatcher`** —
//!   need a background OS thread/subscription and a callback invoked
//!   repeatedly over the object's lifetime; both are user-code re-entry.
//! - **`createReadStream`, `createWriteStream`, `ReadStream`, `WriteStream`,
//!   `Utf8Stream`** — stream classes with `EventEmitter` semantics, same
//!   re-entry problem, and (for `Utf8Stream`) unverified even in Node itself
//!   per the reference doc's own flag.
//! - **`Dir`/`Dirent`/`opendirSync`/`globSync`** — `opendirSync` needs a
//!   cursor object whose `readSync()` method advances hidden state across
//!   calls; this module has no handle table for that (unlike the fd table
//!   above, which only ever answers a fixed function, never a stateful
//!   iterator method built at call time). `globSync` needs a pattern matcher
//!   this crate does not own and was told not to add a dependency for.
//!   `readdirSync` covers flat directory listing without either.
//! - **`utimesSync`, `lutimesSync`, `futimesSync`** — setting file times has
//!   no `std::fs` API; only `libc`/a timestamp crate provides it, and this
//!   module may not add a dependency (`Cargo.toml` is not owned by this
//!   task).
//! - **`chownSync`, `lchownSync`, `fchownSync`, `fchmodSync`, `lchmodSync`**
//!   — POSIX uid/gid/lchmod need `libc`, same as above; not a `std::fs` API
//!   and no dependency may be added.
//! - **`statfsSync`** — filesystem-level statistics need `libc::statvfs`/
//!   Windows `GetDiskFreeSpaceExW`, same dependency limit.
//! - **`mkdtempDisposableSync`** — needs a `{ path, remove(), [Symbol.dispose]() }`
//!   object whose `remove` is itself a bound closure over the generated path;
//!   the value API's `make_callable` takes a bare `extern "C" fn` with no
//!   captured state (see the same limit noted on `stat_sync`'s `is_file_answer`
//!   pattern), so this cannot be built without a second per-call function per
//!   path.
//! - **`openAsBlob`** — promise-only in Node, and needs a global `Blob` this
//!   engine does not have.
//! - **`fs.realpathSync.native`, `fs.F_OK`/`R_OK`/`W_OK`/`X_OK` top-level
//!   deprecated aliases** — `fs.constants.*` is implemented (see
//!   [`constants::install`]); the deprecated top-level aliases are trivial
//!   duplicates and were not worth the four extra `put_member` calls given
//!   everything else in this list still waiting.

mod basic;
mod constants;
mod dirs;
mod fd;
mod links;
mod promises;
mod stats;

use rts_core_rwk::entry::{Context, Provided};

/// The namespace `node:fs` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("readFileSync", basic::read_file_sync),
        ("writeFileSync", basic::write_file_sync),
        ("appendFileSync", basic::append_file_sync),
        ("existsSync", basic::exists_sync),
        ("copyFileSync", basic::copy_file_sync),
        ("renameSync", basic::rename_sync),
        ("unlinkSync", basic::unlink_sync),
        ("mkdirSync", dirs::mkdir_sync),
        ("rmSync", dirs::rm_sync),
        ("rmdirSync", dirs::rmdir_sync),
        ("readdirSync", dirs::readdir_sync),
        ("mkdtempSync", dirs::mkdtemp_sync),
        ("cpSync", dirs::cp_sync),
        ("statSync", stats::stat_sync),
        ("lstatSync", stats::lstat_sync),
        ("fstatSync", fd::fstat_sync),
        ("accessSync", links::access_sync),
        ("readlinkSync", links::readlink_sync),
        ("realpathSync", links::realpath_sync),
        ("symlinkSync", links::symlink_sync),
        ("linkSync", links::link_sync),
        ("chmodSync", links::chmod_sync),
        ("truncateSync", links::truncate_sync),
        ("openSync", fd::open_sync),
        ("closeSync", fd::close_sync),
        ("fsyncSync", fd::fsync_sync),
        ("fdatasyncSync", fd::fdatasync_sync),
        ("ftruncateSync", fd::ftruncate_sync),
    ];
    let namespace = rts_core_rwk::entry::make_namespace(context, members);
    constants::install(context, namespace);
    let promises = promises::namespace(context);
    rts_core_rwk::entry::put_member(context, namespace, "promises", promises);
    namespace
}

/// An argument as text, `None` when absent.
pub(crate) fn text(value: u64) -> Option<String> {
    let absent = rts_core_rwk::entry::undefined_value();
    match value == absent {
        true => None,
        false => rts_core_rwk::entry::text_of(value),
    }
}

/// A string value.
pub(crate) fn string(text: &str) -> u64 {
    rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_string(context, text))
}

/// A boolean value.
pub(crate) fn bool_value(held: bool) -> u64 {
    rts_core_rwk::entry::boolean_value(held)
}

/// A boolean option read off an options object, `false` when the argument is
/// absent, not an object, or does not carry the key.
pub(crate) fn option_flag(options: u64, name: &str) -> bool {
    let absent = rts_core_rwk::entry::undefined_value();
    if options == absent {
        return false;
    }
    let value = rts_core_rwk::entry::with_runtime(|context| {
        rts_core_rwk::entry::get_member(context, options, name)
    });
    value == rts_core_rwk::entry::boolean_value(true)
}

/// A number argument, `None` when absent. `text_of` reads any value's text
/// form (including a number's), so the parse happens here rather than adding
/// a second value-API accessor for what is already reachable this way.
pub(crate) fn number(value: u64) -> Option<f64> {
    // Asked of the value rather than of its text — see `number_of`.
    rts_core_rwk::entry::number_of(value)
}

/// Whether the last operation performed by a member of this module succeeded.
///
/// # Why a side channel and not a return value
///
/// Because these members answer `undefined` in Node whether they worked or not
/// — that is the shape of the synchronous API, and Node reports failure by
/// throwing, which this engine cannot do across a call.
///
/// So the outcome has to reach the caller some other way, and the promise half
/// genuinely needs it: `fs.promises.writeFile` must REJECT where the sync form
/// silently did nothing. Without this it resolved, and a program's `.then`
/// success path ran after a write that never happened — a wrong program that
/// runs, which is the outcome this repository refuses everywhere else.
///
/// This is `errno`, and the resemblance is the argument rather than an
/// accident: a per-thread "what happened last" is what C settled on for exactly
/// this problem, and it has C's rule too — **read it immediately, in the same
/// native, with nothing in between**. A second operation overwrites it.
///
/// Thread-local because a context is: two threads running programs do not share
/// one.
mod outcome {
    use std::cell::Cell;

    thread_local! {
        static LAST: Cell<bool> = const { Cell::new(true) };
    }

    /// Records what an operation answered, and answers `undefined` — which is
    /// what every member using this returns anyway, so the call reads as the
    /// tail of the member rather than as bookkeeping beside it.
    pub(in crate::fs) fn record(succeeded: bool) {
        LAST.with(|held| held.set(succeeded));
    }

    /// Whether it succeeded.
    pub(in crate::fs) fn succeeded() -> bool {
        LAST.with(Cell::get)
    }
}

pub(self) use outcome::{record, succeeded};
