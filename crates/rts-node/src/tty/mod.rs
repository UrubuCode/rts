//! `node:tty` — terminal-device queries and control.
//!
//! # Reuse-check
//!
//! Run before writing, over `crates/`. `rts-cranelift` owns nothing about file
//! descriptors or terminals and `entry/modules.rs` exposes nothing
//! terminal-shaped, so no host surface answers any member here. What the search
//! DID find, and what is reused rather than rewritten:
//!
//! - `crate::fs::options_and_listener` — Node's `(x[, options], callback)`
//!   shift. `cursorTo(x, y?, callback?)` has exactly that shape in its last two
//!   slots, so it calls that function instead of asking a second time whether
//!   slot three is callable.
//! - `entry::make_prototype`/`make_instance` — the pairing `events.rs` and
//!   `fs/handle.rs` already use to give a class ONE shared prototype, which is
//!   what `instanceof` and a program-added method both depend on.
//! - The old engine's `rts-node/src/tty/detect.rs` answers `isatty` and the
//!   color heuristic. It cannot be called (it is the engine being replaced and
//!   reaches `rts_engine::Engine`), so its DECISIONS are reused and named where
//!   they land: see the reuse-check in [`colors`] and in [`platform`].
//!
//! # Layout
//!
//! `platform` is the per-OS ladder (is-a-terminal, window size, raw mode, write
//! to an fd); `colors` is the environment heuristic; this file is the JS shape —
//! namespace, two classes, argument reading, callbacks. The seams are by
//! concern: nothing in `platform` knows what a value is, and nothing here has a
//! `#[cfg]`.
//!
//! # The borrow rule, as it applies here
//!
//! Every method reads `this.fd` under one borrow, DROPS it, then does the OS
//! call and finally invokes the caller's callback — because `entry::call` is
//! ambient and taking a second borrow from an `extern "C"` frame aborts the
//! process rather than panicking. That ordering is the whole reason these
//! methods are not written as one `with_runtime` body.
//!
//! # Not implemented, by name
//!
//! - The `'resize'` event — needs a `SIGWINCH` handler (POSIX) or a background
//!   `ReadConsoleInputW` thread (Windows) AND an event loop that keeps polling
//!   while a listener is attached, which §5.7 of the spec flags as a shared-infra
//!   gap this module cannot close alone. Consequently `columns`/`rows` are a
//!   SNAPSHOT taken when the stream was constructed and never move;
//!   `getWindowSize()` is the live query and is what a resizable UI must call.
//!   Answering `undefined` for both when the fd has no window is deliberate — a
//!   plausible `80`/`24` is the wrong answer that runs.
//! - The legacy-Windows cursor path (`SetConsoleCursorPosition`,
//!   `FillConsoleOutputCharacterW`, `ScrollConsoleScreenBufferW`) — a console
//!   that refuses `ENABLE_VIRTUAL_TERMINAL_PROCESSING` gets NOTHING written and
//!   a `false` return, rather than literal `\x1b[2K` text on the user's screen.
//! - Raw-mode restoration at process exit (§4's security note) — needs an exit
//!   and signal hook this crate has no accepted dependency for. A program that
//!   enables raw mode and dies leaves the terminal raw.
//! - `net.Socket` ancestry — both classes are documented to extend it, and this
//!   crate's `node:net` has no stream base to extend. So there is no `.write()`,
//!   `.pipe()`, `'data'`, `'drain'` or real backpressure here, and the `boolean`
//!   these methods answer means "the bytes were written", never "keep going" —
//!   the standalone-wrapper posture §5.6/§7 name as the accepted v1 shape.
//! - `clearLine` with a `dir` outside `{-1, 0, 1}` writes nothing and answers
//!   `false`. Node throws `ERR_OUT_OF_RANGE`; `entry::throw` ends the PROGRAM
//!   rather than raising a catchable error, so it is not the `throw` this needs.

mod colors;
mod platform;

use rts_core::entry::{self, Context, Provided};

/// The namespace `node:tty` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("isatty", isatty),
        ("ReadStream", make_read_stream),
        ("WriteStream", make_write_stream),
    ];
    let namespace = entry::make_namespace(context, members);
    // "tty.ReadStream"/"tty.WriteStream" rather than the bare names: `node:fs`
    // registers ITS OWN `"ReadStream"`/`"WriteStream"` under those same bare
    // names, with a different method table, and `make_prototype` is idempotent
    // by name — see its doc comment for the collision this otherwise is.
    let read_prototype = entry::make_prototype(context, "tty.ReadStream", READ_STREAM_METHODS);
    let read_ctor = entry::get_member(context, namespace, "ReadStream");
    entry::put_member(context, read_ctor, "prototype", read_prototype);
    let write_prototype = entry::make_prototype(context, "tty.WriteStream", WRITE_STREAM_METHODS);
    let write_ctor = entry::get_member(context, namespace, "WriteStream");
    entry::put_member(context, write_ctor, "prototype", write_prototype);
    namespace
}

/// `tty.isatty(fd)` — never throws; a malformed `fd` answers `false`.
extern "C" fn isatty(_e: u64, _this: u64, fd: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::boolean_value(descriptor(fd).is_some_and(platform::is_terminal))
}

/// A value read as a file descriptor: a non-negative whole number, or nothing.
///
/// `number_of` is the value's own numeric view rather than `to_number`, so a
/// string `"1"` is refused instead of quietly becoming fd 1 — asking WHAT an
/// argument is, not converting it.
fn descriptor(value: u64) -> Option<i64> {
    let number = entry::number_of(value)?;
    match number >= 0.0 && number.fract() == 0.0 && number <= i64::MAX as f64 {
        true => Some(number as i64),
        false => None,
    }
}

/// The fd a stream instance was built over.
fn stream_fd(this: u64) -> Option<i64> {
    let held = entry::with_runtime(|context| entry::get_member(context, this, "fd"));
    descriptor(held)
}

const READ_STREAM_METHODS: &[(&str, Provided)] = &[("setRawMode", set_raw_mode)];

/// `new tty.ReadStream(fd, options?)`. Not the general `net.Socket`
/// construction real Node performs (see the module doc) — records `fd` and the
/// always-`true` `isTTY`/initially-`false` `isRaw` shape (§4 of the spec:
/// `isTTY` is a class constant, not a live `isatty(fd)` re-check, even here).
extern "C" fn make_read_stream(_e: u64, this: u64, fd: u64, _options: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "tty.ReadStream", READ_STREAM_METHODS);
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

/// `readStream.setRawMode(mode)` — chainable, answers `this`.
///
/// `isRaw` moves only when the OS accepted the change, so the property keeps
/// reporting what the device is actually in rather than what was asked for. A
/// non-TTY fd therefore leaves it `false`, which is the honest reading: Node
/// throws there, and `entry::throw` ends the program instead of raising
/// something a caller could catch (see the module doc).
extern "C" fn set_raw_mode(_e: u64, this: u64, mode: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let wanted = entry::with_runtime(|context| entry::to_boolean_in(context, mode));
    let Some(fd) = stream_fd(this) else {
        return this;
    };
    if platform::set_raw_mode(fd, wanted) {
        let value = entry::boolean_value(wanted);
        entry::with_runtime(|context| entry::put_member(context, this, "isRaw", value));
    }
    this
}

const WRITE_STREAM_METHODS: &[(&str, Provided)] = &[
    ("getColorDepth", get_color_depth),
    ("hasColors", has_colors),
    ("getWindowSize", get_window_size),
    ("clearLine", clear_line),
    ("clearScreenDown", clear_screen_down),
    ("cursorTo", cursor_to),
    ("moveCursor", move_cursor),
];

/// `new tty.WriteStream(fd)`.
extern "C" fn make_write_stream(_e: u64, this: u64, fd: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    // The geometry is read BEFORE the borrow: `window_size` is an OS call, and
    // taking one inside `with_runtime` is fine, but reading `fd` back out of the
    // half-built instance to do it afterwards would be a second borrow for a
    // number already in hand.
    let size = descriptor(fd).and_then(platform::window_size);
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "tty.WriteStream", WRITE_STREAM_METHODS);
        let instance = match entry::is_object(context, this) {
            true => this,
            false => entry::make_instance(context, prototype),
        };
        entry::put_member(context, instance, "fd", fd);
        let is_tty = entry::boolean_value(true);
        entry::put_member(context, instance, "isTTY", is_tty);
        let (columns, rows) = match size {
            Some((columns, rows)) => (
                entry::make_number(f64::from(columns)),
                entry::make_number(f64::from(rows)),
            ),
            // No window to measure — see the module doc's refusal of `'resize'`
            // for why this is `undefined` and not a plausible `80`/`24`.
            None => (entry::undefined_in(context), entry::undefined_in(context)),
        };
        entry::put_member(context, instance, "columns", columns);
        entry::put_member(context, instance, "rows", rows);
        instance
    })
}

/// `writeStream.getWindowSize()` — `[columns, rows]`, live rather than the
/// snapshot the properties hold.
///
/// `undefined` when the fd has no window, which is the one place this diverges
/// from Node's signature: Node's own tuple is whatever its internal call
/// produced, and there is no honest tuple to answer when the query failed.
extern "C" fn get_window_size(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some((columns, rows)) = stream_fd(this).and_then(platform::window_size) else {
        return entry::undefined_value();
    };
    entry::with_runtime(|context| {
        let values = vec![
            entry::make_number(f64::from(columns)),
            entry::make_number(f64::from(rows)),
        ];
        entry::make_array_in(context, values)
    })
}

/// `writeStream.getColorDepth(env?)` — pure heuristic, no capability query (see
/// [`colors`]). Returns `1`/`4`/`8`/`24`.
extern "C" fn get_color_depth(_e: u64, this: u64, env: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::make_number(f64::from(colors::depth(env, stream_fd(this))))
}

/// `writeStream.hasColors(count?, env?)` / `hasColors(env?)`.
extern "C" fn has_colors(_e: u64, this: u64, a0: u64, a1: u64, _a2: u64, _a3: u64) -> u64 {
    // The two-shape overload: a numeric first argument is `count` and `env` is
    // second; otherwise the first IS `env` (or absent) and `count` defaults.
    let (count, env) = match entry::number_of(a0) {
        Some(number) if number >= 0.0 => (number.min(f64::from(u32::MAX)) as u32, a1),
        _ => (16, a0),
    };
    entry::boolean_value(colors::has(count, env, stream_fd(this)))
}

/// `writeStream.clearLine(dir, callback?)`.
extern "C" fn clear_line(_e: u64, this: u64, dir: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    let sequence = match entry::number_of(dir) {
        Some(number) if number == -1.0 => "\x1b[1K",
        Some(number) if number == 1.0 => "\x1b[0K",
        Some(number) if number == 0.0 => "\x1b[2K",
        // Out of range: nothing is written. See the module doc.
        _ => return finish(callback, false),
    };
    emit(this, sequence, callback)
}

/// `writeStream.clearScreenDown(callback?)`.
extern "C" fn clear_screen_down(_e: u64, this: u64, callback: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    emit(this, "\x1b[0J", callback)
}

/// `writeStream.cursorTo(x, y?, callback?)` / `cursorTo(x, callback)`.
///
/// The overload is `crate::fs::options_and_listener`'s, reused rather than
/// re-decided: it answers "is the second of these two slots the callback, or is
/// the first?", which is the identical question `fs.watch` asks.
extern "C" fn cursor_to(_e: u64, this: u64, x: u64, y: u64, callback: u64, _a3: u64) -> u64 {
    let (y, callback) = crate::fs::options_and_listener(y, callback);
    let Some(column) = entry::number_of(x).filter(|number| number.is_finite()) else {
        return finish(callback, false);
    };
    // ANSI positions are 1-based and Node's arguments are 0-based; a negative
    // argument would underflow the conversion, so it is clamped rather than
    // wrapped into a position on the far side of the screen.
    let column = column.max(0.0) as u64 + 1;
    let sequence = match entry::number_of(y).filter(|number| number.is_finite()) {
        Some(row) => format!("\x1b[{};{column}H", row.max(0.0) as u64 + 1),
        None => format!("\x1b[{column}G"),
    };
    emit(this, &sequence, callback)
}

/// `writeStream.moveCursor(dx, dy, callback?)` — RELATIVE, unlike
/// [`cursor_to`].
extern "C" fn move_cursor(_e: u64, this: u64, dx: u64, dy: u64, callback: u64, _a3: u64) -> u64 {
    let horizontal = entry::number_of(dx).filter(|number| number.is_finite()).unwrap_or(0.0);
    let vertical = entry::number_of(dy).filter(|number| number.is_finite()).unwrap_or(0.0);
    let mut sequence = String::new();
    if horizontal != 0.0 {
        let letter = match horizontal > 0.0 {
            true => 'C',
            false => 'D',
        };
        sequence.push_str(&format!("\x1b[{}{letter}", horizontal.abs() as u64));
    }
    if vertical != 0.0 {
        let letter = match vertical > 0.0 {
            true => 'B',
            false => 'A',
        };
        sequence.push_str(&format!("\x1b[{}{letter}", vertical.abs() as u64));
    }
    // Both deltas zero is a no-op that SUCCEEDED — the cursor is where it was
    // asked to be — so it answers `true` without a write.
    if sequence.is_empty() {
        return finish(callback, true);
    }
    emit(this, &sequence, callback)
}

/// Writes a control sequence to the stream's fd and settles the callback.
fn emit(this: u64, sequence: &str, callback: u64) -> u64 {
    let written = stream_fd(this).is_some_and(|fd| platform::write_all(fd, sequence.as_bytes()));
    finish(callback, written)
}

/// Invokes the optional callback and answers what the method returns.
///
/// Called with no arguments and immediately, not on a microtask: this module
/// cannot mint one, and the write already happened synchronously — a callback
/// deferred to a tick that never comes would be worse than an early one.
///
/// The callback runs with NO borrow held, which is the rule the module doc
/// states: `entry::call` is ambient and a second borrow from an `extern "C"`
/// frame aborts the process.
fn finish(callback: u64, ok: bool) -> u64 {
    let callable = entry::with_runtime(|context| entry::is_callable_in(context, callback));
    if callable {
        let absent = entry::undefined_value();
        entry::call(callback, absent, absent, absent, absent, absent);
    }
    entry::boolean_value(ok)
}
