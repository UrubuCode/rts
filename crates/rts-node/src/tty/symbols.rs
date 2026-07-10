//! node:tty — base extern "C" symbol implementations (the sync surface).
//!
//! This slice covers the pure `number`→`boolean` surface — `isatty(fd)`.
//! `ReadStream`/`WriteStream` (the `tty.ReadStream`/`tty.WriteStream` classes
//! Node exposes for `process.stdin`/`stdout`) are DEFERRED: they are stateful
//! stream objects (need handles + the stream/event-emitter machinery), out of
//! scope for this pure-function slice.
//!
//! ABI mirrors the pure-namespace shape used across RTS: the `fd` arg arrives
//! as `i64` (`AbiType::I64`); the result is `i64` 0/1 (`AbiType::Bool`).
//! Symbols follow the rts-node convention `__RTS_FN_NODE_TTY_*`.

use std::io::IsTerminal;

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
