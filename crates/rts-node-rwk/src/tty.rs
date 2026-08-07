//! `node:tty` — terminal-device queries and control.
//!
//! # Reuse-check
//!
//! `reuse-check` was run before writing this: `rts-cranelift` owns nothing
//! about file descriptors or terminals, and `entry/modules.rs` exposes
//! nothing terminal-shaped. The one primitive reused across every member
//! here is `make_prototype`/`make_instance`, the same pairing
//! `events.rs`'s `EventEmitter` and `fs/handle.rs`'s `FileHandle` already
//! use to give `ReadStream`/`WriteStream` one shared prototype per class.
//!
//! # What `std` alone can answer
//!
//! `isatty` is `std::io::IsTerminal`, stable since Rust 1.70 — zero new
//! dependency, and (per `docs/reference/node/tty.md` §5.1) the preferred
//! primitive over hand-rolling `libc::isatty`/`GetConsoleMode`. Wrapping an
//! arbitrary numeric fd/HANDLE for it needs one `unsafe` per platform (the
//! fd is not proven open or TTY-backed before the call) — the same shape of
//! unsafety `fs/perms.rs` already carries for its own libc calls in this
//! crate, not a new category introduced here.
//!
//! Terminal geometry (`columns`/`rows`/`getWindowSize`), raw mode, and
//! cursor/clear escape sequences all need a platform ioctl/Console-API call
//! this crate has no accepted dependency for (`libc`'s `termios`/`ioctl`
//! bindings exist here, but `TIOCGWINSZ`/`tcsetattr`'s raw-mode flag work
//! and the Windows `GetConsoleMode`/`SetConsoleScreenBufferInfo` family are
//! not something this task adds — see "Not implemented", below).
//!
//! # `getColorDepth`/`hasColors`: honest heuristic, not a real query
//!
//! Per the spec (§5.1), Node's own version is an env-var heuristic with no
//! OS call behind it at all — so this is not a divergence to name, it is
//! the one member of `WriteStream` this crate can implement completely: it
//! reads `FORCE_COLOR`/`NO_COLOR`/`NODE_DISABLE_COLORS` from the same
//! `env` map Node's own does, or from `std::env::var` when no `env`
//! argument is given, and answers a number/boolean from that alone.
//!
//! # Not implemented, by name
//!
//! - `columns`/`rows`/`getWindowSize()` — needs `TIOCGWINSZ` (POSIX) or
//!   `GetConsoleScreenBufferInfo` (Windows); `std` has no cross-platform
//!   window-size query and this task adds no ioctl/Console-API wrapper for
//!   one. Reading `undefined` here is the honest answer — never a
//!   plausible `80`/`24`, which is exactly the wrong-shaped guess the
//!   `node:os` module's own doc already refuses for `cpus()`'s clock speed.
//! - `setRawMode`/`isRaw` — needs `termios`/`GetConsoleMode` raw-mode
//!   toggling, not present without the same missing platform layer.
//! - `clearLine`/`clearScreenDown`/`cursorTo`/`moveCursor` — each needs to
//!   WRITE to the fd (an ANSI escape sequence, or the Windows legacy
//!   Console API as a fallback); this module reads state, it does not open
//!   a raw write path to an arbitrary fd, which none of `std`'s safe API
//!   surface offers for a bare fd number.
//! - The `'resize'` event — needs a `SIGWINCH` handler or a background
//!   Windows console-event thread; no signal-handling dependency is
//!   accepted here.
//! - `net.Socket` ancestry (`ReadStream`/`WriteStream` are documented to
//!   extend it) — `node:net`'s stream base does not exist in this crate;
//!   both classes below are plain objects with only the members this file
//!   implements, exactly the "standalone wrapper" posture the spec's own
//!   §5.6/§7 name as the accepted v1 shape.

use rts_core_rwk::entry::{self, Context, Provided};
use std::io::IsTerminal;

/// The namespace `node:tty` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("isatty", isatty),
        ("ReadStream", make_read_stream),
        ("WriteStream", make_write_stream),
    ];
    let namespace = entry::make_namespace(context, members);
    let read_prototype = entry::make_prototype(context, "ReadStream", READ_STREAM_METHODS);
    let read_ctor = entry::get_member(context, namespace, "ReadStream");
    entry::put_member(context, read_ctor, "prototype", read_prototype);
    let write_prototype = entry::make_prototype(context, "WriteStream", WRITE_STREAM_METHODS);
    let write_ctor = entry::get_member(context, namespace, "WriteStream");
    entry::put_member(context, write_ctor, "prototype", write_prototype);
    namespace
}

/// `tty.isatty(fd)` — never throws; a malformed `fd` answers `false`.
extern "C" fn isatty(_e: u64, _this: u64, fd: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::boolean_value(is_a_tty(fd))
}

fn is_a_tty(fd_value: u64) -> bool {
    let Some(number) = entry::number_of(fd_value) else {
        return false;
    };
    if number < 0.0 || number.fract() != 0.0 {
        return false;
    }
    fd_is_terminal(number as i64)
}

#[cfg(unix)]
fn fd_is_terminal(fd: i64) -> bool {
    use std::os::fd::{BorrowedFd, RawFd};
    let Ok(raw) = RawFd::try_from(fd) else {
        return false;
    };
    // SAFETY: the fd is not proven open here — an invalid one makes
    // `is_terminal()` answer `false` rather than any call actually
    // dereferencing memory, which is the same "no throw" contract
    // `isatty(3)` itself has. No ownership is taken: the borrow is dropped
    // before returning, and the underlying fd is never closed by it.
    let borrowed = unsafe { BorrowedFd::borrow_raw(raw) };
    borrowed.is_terminal()
}

#[cfg(windows)]
fn fd_is_terminal(fd: i64) -> bool {
    use std::os::windows::io::{BorrowedHandle, RawHandle};
    // The Windows C runtime's `_get_osfhandle` turns a CRT fd number (the
    // numbering `0`/`1`/`2` and anything `libc::open`-like hands back) into
    // the underlying `HANDLE` `IsTerminal` needs. `libc` is an already
    // accepted dependency in this crate (`Cargo.toml`, for `fs/perms.rs`'s
    // chown/utimensat calls) — no new one is added here.
    let handle = unsafe { libc::get_osfhandle(fd as libc::c_int) };
    if handle == -1isize as libc::intptr_t {
        return false;
    }
    // SAFETY: `_get_osfhandle` answers a borrowed view of the CRT's own
    // handle table; it is not owned or closed here, matching the promise
    // `BorrowedHandle` documents. An invalid `handle` value was already
    // rejected above.
    let borrowed = unsafe { BorrowedHandle::borrow_raw(handle as RawHandle) };
    borrowed.is_terminal()
}

const READ_STREAM_METHODS: &[(&str, Provided)] = &[("setRawMode", set_raw_mode_unavailable)];

/// `new tty.ReadStream(fd, options?)`. Not the general `net.Socket`
/// construction real Node performs (see the module doc) — records `fd` and
/// the always-`true` `isTTY`/always-`false` `isRaw` class-constant shape
/// (§4 of the spec: `isTTY` is a class constant, not a live `isatty(fd)`
/// re-check, even here).
extern "C" fn make_read_stream(_e: u64, this: u64, fd: u64, _options: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "ReadStream", READ_STREAM_METHODS);
        let instance = match entry::is_object(context, this) {
            true => this,
            false => entry::make_instance(context, prototype),
        };
        entry::put_member(context, instance, "fd", fd);
        let is_tty = entry::boolean_value(true);
        entry::put_member(context, instance, "isTTY", is_tty);
        let is_raw = entry::boolean_value(false);
        entry::put_member(context, instance, "isRaw", is_raw);
        instance
    })
}

/// `readStream.setRawMode(mode)` — refused: see the module doc's "Not
/// implemented" section. Answers `this` unchanged rather than `undefined`,
/// so a chained `.setRawMode(true).on(...)` at least does not crash on a
/// method call to a missing member — but `isRaw` never moves off `false`.
extern "C" fn set_raw_mode_unavailable(_e: u64, this: u64, _mode: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    this
}

const WRITE_STREAM_METHODS: &[(&str, Provided)] = &[
    ("getColorDepth", get_color_depth),
    ("hasColors", has_colors),
];

/// `new tty.WriteStream(fd)`.
extern "C" fn make_write_stream(_e: u64, this: u64, fd: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "WriteStream", WRITE_STREAM_METHODS);
        let instance = match entry::is_object(context, this) {
            true => this,
            false => entry::make_instance(context, prototype),
        };
        entry::put_member(context, instance, "fd", fd);
        let is_tty = entry::boolean_value(true);
        entry::put_member(context, instance, "isTTY", is_tty);
        // `columns`/`rows`: see "Not implemented" — `undefined` rather than
        // a plausible `80`/`24`.
        let unknown = entry::undefined_in(context);
        entry::put_member(context, instance, "columns", unknown);
        entry::put_member(context, instance, "rows", unknown);
        instance
    })
}

/// `writeStream.getColorDepth(env?)` — pure heuristic, no OS call (see the
/// module doc). Returns `1`/`4`/`8`/`24`.
extern "C" fn get_color_depth(_e: u64, _this: u64, env: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::make_number(f64::from(color_depth(env)))
}

/// `writeStream.hasColors(count?, env?)` / `hasColors(env?)`.
extern "C" fn has_colors(_e: u64, _this: u64, a0: u64, a1: u64, _a2: u64, _a3: u64) -> u64 {
    // The two-shape overload: if `a0` is a number, it is `count` and `env`
    // is `a1`; otherwise `a0` itself is `env` (or absent) and `count`
    // defaults to `16`.
    let (count, env) = match entry::number_of(a0) {
        Some(number) => (number.max(2.0) as u32, a1),
        None => (16, a0),
    };
    let depth = color_depth(env);
    let supported: u32 = match depth {
        1 => 2,
        4 => 16,
        8 => 256,
        _ => 16_777_216,
    };
    entry::boolean_value(supported >= count)
}

/// One environment read, `env[name]` if `env` is an object, else
/// `std::env::var(name)`.
fn env_var(env: u64, name: &str) -> Option<String> {
    let absent = entry::undefined_value();
    if env == absent {
        return std::env::var(name).ok();
    }
    // `string_in`, not `text_of`. The second is `ToString`, so the `undefined`
    // of an absent key answered `Some("undefined")` — every key of every env
    // object looked present, and `getColorDepth` short-circuited on `NO_COLOR`
    // every time. The same coercion-mistaken-for-a-test that made
    // `node:worker_threads` cross every number as a string.
    entry::with_runtime(|context| {
        let value = entry::get_member(context, env, name);
        entry::string_in(context, value)
    })
}

/// The `getColorDepth`/`hasColors` heuristic — `FORCE_COLOR`, `NO_COLOR`,
/// `NODE_DISABLE_COLORS`, per the spec's documented precedence. Does not
/// consult `TERM`/`COLORTERM`/CI-detection variables: the spec (§7) flags
/// Node's exact extended heuristic as unverified even for itself, and this
/// task adds none of that guesswork — only the three variables the public
/// docs commit to.
fn color_depth(env: u64) -> u8 {
    if env_var(env, "NO_COLOR").is_some() || env_var(env, "NODE_DISABLE_COLORS").is_some() {
        return 1;
    }
    if let Some(force) = env_var(env, "FORCE_COLOR") {
        return match force.as_str() {
            "0" => 1,
            "1" => 4,
            "2" => 8,
            "3" => 24,
            _ => 4,
        };
    }
    1
}
