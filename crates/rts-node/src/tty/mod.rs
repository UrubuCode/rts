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

mod detect;

use rts_engine::AbiType::{self, Bool, I32};
use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// `tty.isatty(fd)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_TTY_ISATTY(fd: i32) -> i64 {
    detect::isatty(fd) as i64
}

/// `tty.getColorDepth(fd)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_TTY_GET_COLOR_DEPTH(fd: i32) -> i32 {
    detect::color_depth(fd) as i32
}

/// `tty.hasColors(count, fd)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_TTY_HAS_COLORS(count: i32, fd: i32) -> i64 {
    detect::has_colors(count.max(0) as u32, fd) as i64
}

fn f(name: &str, symbol: &str, args: Vec<AbiType>, ret: AbiType, ts: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig: Sig::new(args, ret),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        pure: false,
        emit: None,
    }
}

/// Registers the `node:tty` surface.
pub fn register(e: &mut Engine) {
    e.ns("node:tty")
        .doc("Terminal detection (node:tty): isatty, getColorDepth, hasColors.")
        .member(f("isatty", "__RTS_FN_NODE_TTY_ISATTY", vec![I32], Bool, "isatty(fd: number): boolean", __RTS_FN_NODE_TTY_ISATTY as *const u8))
        .member(f("getColorDepth", "__RTS_FN_NODE_TTY_GET_COLOR_DEPTH", vec![I32], I32, "getColorDepth(fd?: number): number", __RTS_FN_NODE_TTY_GET_COLOR_DEPTH as *const u8))
        .member(f("hasColors", "__RTS_FN_NODE_TTY_HAS_COLORS", vec![I32, I32], Bool, "hasColors(count: number, fd?: number): boolean", __RTS_FN_NODE_TTY_HAS_COLORS as *const u8))
        .done();
}
