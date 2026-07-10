//! `node:tty` — TTY detection.
//!
//! Native rts-node implementation (no rts-std mirror). This slice covers the
//! pure `number`→`boolean` surface — `isatty(fd)`. `ReadStream`/`WriteStream`
//! (the `tty.ReadStream`/`tty.WriteStream` classes Node exposes for
//! `process.stdin`/`stdout`) are DEFERRED: they are stateful stream objects
//! (need handles + the stream/event-emitter machinery), out of scope for this
//! pure-function slice.
//!
//! ABI mirrors the pure-namespace shape used across RTS: the `fd` arg arrives
//! as `i64` (`AbiType::I64`); the result is `i64` 0/1 (`AbiType::Bool`).
//! Symbols follow the rts-node convention `__RTS_FN_NODE_TTY_*`.

use std::io::IsTerminal;

use rts_engine::{sig, Engine, FnPtr, Member, MemberFlags, MemberKind};

/// `tty.isatty(fd)` — true when `fd` refers to a TTY. Node maps the standard
/// fds 0/1/2 to stdin/stdout/stderr; any other fd is not a recognized stream
/// here, so it reports `false` (Node would consult the OS for arbitrary fds,
/// which this pure slice does not have a handle-backed fd table to do).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_TTY_ISATTY(fd: i64) -> i64 {
    let is_tty = match fd {
        0 => std::io::stdin().is_terminal(),
        1 => std::io::stdout().is_terminal(),
        2 => std::io::stderr().is_terminal(),
        _ => false,
    };
    if is_tty {
        1
    } else {
        0
    }
}

fn pure_func(
    name: &str,
    symbol: &str,
    sig: rts_engine::Sig,
    ts: &str,
    doc: &str,
    fp: *const u8,
) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: true,
        intrinsic: None,
    }
}

/// Registers the `node:tty` surface into the engine Registry.
pub fn register(e: &mut Engine) {
    e.module("node", "tty")
        .alias("tty")
        .doc("TTY detection (node:tty).")
        .member(pure_func(
            "isatty",
            "__RTS_FN_NODE_TTY_ISATTY",
            sig!(I64 => Bool),
            "isatty(fd: number): boolean",
            "True when fd (0/1/2 => stdin/stdout/stderr) refers to a TTY.",
            __RTS_FN_NODE_TTY_ISATTY as *const u8,
        ))
        .done();
}
