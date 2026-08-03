//! `node:tty` — terminal-device access. The standalone, stream-independent
//! surface: `isatty(fd)` (real fd query) and the color-capability detection
//! `getColorDepth(fd)`/`hasColors(count, fd)` (computed from the live
//! environment). Every value comes from a real syscall or the actual env — no
//! fabricated capabilities.
//!
//! Node exposes `getColorDepth`/`hasColors` as `tty.WriteStream` methods; RTS
//! surfaces them as module functions taking the fd explicitly (the values are
//! genuine, the access is a call).
//!
//! Deferred (both TTY classes extend `net.Socket` — the stream/socket layer,
//! tty.md §2.1): `ReadStream`/`WriteStream`, `setRawMode`/`isRaw`,
//! `columns`/`rows`/`getWindowSize`, `clearLine`/`clearScreenDown`/`cursorTo`/
//! `moveCursor` (they write escape sequences to a WriteStream), the `'resize'`
//! event.
//!
//! Layout: `detect` (syscall + env algorithm), `mod` (registration).
//!
//! # Authoring: `#[rtse::function]`, not a hand-built `Member`
//!
//! Each function DECLARES its module and JS name; everything else is DERIVED
//! from the Rust signature — the linker symbol (via `rts_abi::scope`), the
//! `AbiType`s, the TS signature, and the doc (from the doc-comment). The old
//! form spelled all of that a second time, in a `.member(f(name, symbol, args,
//! ret, ts, fn_ptr))` row that nothing checked against the function it claimed
//! to describe. See docs/engine/architecture.md.

mod detect;

use rts_engine::Engine;

/// `tty.isatty(fd)` — whether `fd` refers to a terminal.
#[rtse::function(module = "node:tty", value = "isatty")]
fn isatty(fd: i32) -> bool {
    detect::isatty(fd)
}

/// `tty.getColorDepth(fd)` — bit depth of color support on `fd`.
#[rtse::function(module = "node:tty", value = "getColorDepth")]
fn get_color_depth(#[default(1)] fd: i32) -> i32 {
    detect::color_depth(fd) as i32
}

/// `tty.hasColors(count, fd)` — whether `fd` supports at least `count` colors.
#[rtse::function(module = "node:tty", value = "hasColors")]
fn has_colors(count: i32, #[default(1)] fd: i32) -> bool {
    detect::has_colors(count.max(0) as u32, fd)
}

/// Registers the `node:tty` surface.
pub fn register(e: &mut Engine) {
    e.module("node:tty", |m| {
        m.doc("Terminal detection (node:tty): isatty, getColorDepth, hasColors.");
        m.registry(isatty_entry());
        m.registry(get_color_depth_entry());
        m.registry(has_colors_entry());
    });
}
