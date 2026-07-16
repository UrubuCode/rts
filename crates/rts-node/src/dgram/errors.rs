//! node:dgram — Node's error surface: the `ERR_SOCKET_*` codes, the raw errno
//! codes an unbound/failed socket op reports, and the mapping from a real
//! `std::io::Error` to the code Node prints. Every message here is produced from
//! a REAL failed operation — nothing is fabricated.
//!
//! Codes per docs/node-implementation/dgram.md §4 ("Error code reference table").

use std::io;

unsafe extern "C" {
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
    /// The VALUE twin of the throw: an error is not always thrown — a socket
    /// error goes to a callback or the `'error'` listener.
    fn __rtsadp_make_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64) -> u64;
}

/// `connect()` on an already-connected socket.
pub const IS_CONNECTED: &str = "ERR_SOCKET_DGRAM_IS_CONNECTED";
/// `disconnect()`/`remoteAddress()` on an unbound or unconnected socket.
pub const NOT_CONNECTED: &str = "ERR_SOCKET_DGRAM_NOT_CONNECTED";
/// Any operation on a closed socket.
pub const NOT_RUNNING: &str = "ERR_SOCKET_DGRAM_NOT_RUNNING";
/// `send()` with no resolvable port.
pub const BAD_PORT: &str = "ERR_SOCKET_BAD_PORT";
/// A buffer-size get/set on an unbound socket (Node's own class, not an errno).
pub const BUFFER_SIZE: &str = "ERR_SOCKET_BUFFER_SIZE";
/// An op that needs a bound socket (`address()`, `setTTL()`, …).
pub const EBADF: &str = "EBADF";

/// The JS Error CLASS a dgram error code is an instance of — Node's own choice
/// per code (an `ERR_SOCKET_BAD_PORT` really is a `RangeError` there, a bad
/// `type` really is a `TypeError`); everything else — the `ERR_SOCKET_*` states
/// and the raw errnos, which Node raises as `SystemError` — is a plain `Error`.
///
/// This has to be a real class name: the engine fabricates the instance on that
/// class's registered shape, and an unregistered name degrades to throwing a
/// bare string, which has no `.message` for `catch (e)` to read.
fn class_of(code: &str) -> &'static str {
    match code {
        "ERR_SOCKET_BAD_TYPE" | "ERR_INVALID_ARG_TYPE" | "ERR_INVALID_ARG_VALUE" => "TypeError",
        BAD_PORT | "ERR_OUT_OF_RANGE" => "RangeError",
        _ => "Error",
    }
}

/// The `<CODE>: <detail>` message text. The code rides the message because the
/// engine's Error shape has no `code` slot of its own (`message`/`name`/`stack`
/// only) — so `e.message.startsWith('EBADF')` is how a caller reads the code,
/// where Node would offer `e.code`. Documented in dgram.md §8.
fn text(code: &str, message: &str) -> String {
    format!("{code}: {message}")
}

/// Throw a REAL Error-family instance for a dgram error code.
pub fn throw(code: &str, message: &str) {
    let class = class_of(code);
    let msg = text(code, message);
    unsafe {
        __rtsadp_throw_js_error(class.as_ptr(), class.len() as i64, msg.as_ptr(), msg.len() as i64);
    }
}

/// The same error as a VALUE (for a callback's err-first argument / the
/// `'error'` event) instead of a throw.
pub fn value(code: &str, message: &str) -> u64 {
    let class = class_of(code);
    let msg = text(code, message);
    unsafe { __rtsadp_make_js_error(class.as_ptr(), class.len() as i64, msg.as_ptr(), msg.len() as i64) }
}

/// `EBADF` — the socket is not bound yet.
pub fn throw_unbound() {
    throw(EBADF, "bad file descriptor");
}

/// `ERR_SOCKET_DGRAM_NOT_RUNNING` — the socket is closed.
pub fn throw_not_running() {
    throw(NOT_RUNNING, "Not running");
}

/// The Node error code for a socket-op `io::Error`. Kind first (portable), then
/// the raw OS errno for the cases `ErrorKind` does not distinguish — Windows
/// (WSA*) and POSIX values are both mapped, since RTS builds for both.
pub fn code_for(e: &io::Error) -> &'static str {
    use io::ErrorKind::*;
    match e.kind() {
        AddrInUse => return "EADDRINUSE",
        AddrNotAvailable => return "EADDRNOTAVAIL",
        PermissionDenied => return "EACCES",
        ConnectionRefused => return "ECONNREFUSED",
        ConnectionReset => return "ECONNRESET",
        NotConnected => return "ENOTCONN",
        InvalidInput => return "EINVAL",
        TimedOut => return "ETIMEDOUT",
        WouldBlock => return "EAGAIN",
        Unsupported => return "ENOTSUP",
        _ => {}
    }
    match e.raw_os_error() {
        // Windows (WinSock).
        Some(10009) => "EBADF",
        Some(10013) => "EACCES",
        Some(10022) => "EINVAL",
        Some(10040) => "EMSGSIZE",
        Some(10042) => "ENOPROTOOPT",
        Some(10043) => "EPROTONOSUPPORT",
        Some(10047) => "EAFNOSUPPORT",
        Some(10048) => "EADDRINUSE",
        Some(10049) => "EADDRNOTAVAIL",
        Some(10051) => "ENETUNREACH",
        Some(10057) => "ENOTCONN",
        // POSIX.
        Some(9) => "EBADF",
        Some(13) => "EACCES",
        Some(22) => "EINVAL",
        Some(90) => "EMSGSIZE",
        Some(92) => "ENOPROTOOPT",
        Some(93) => "EPROTONOSUPPORT",
        Some(97) => "EAFNOSUPPORT",
        Some(98) => "EADDRINUSE",
        Some(99) => "EADDRNOTAVAIL",
        Some(101) => "ENETUNREACH",
        Some(107) => "ENOTCONN",
        _ => "UNKNOWN",
    }
}

/// `<CODE>: <os message>, <op>` — the shape Node prints for a failed socket op.
pub fn message_for(e: &io::Error, op: &str) -> (String, String) {
    let code = code_for(e).to_string();
    (code, format!("{e}, {op}"))
}

/// Throw a socket-op `io::Error` synchronously (the ops Node documents as
/// throwing: `setTTL`, `addMembership`, buffer sizes, …).
pub fn throw_io(e: &io::Error, op: &str) {
    let (code, msg) = message_for(e, op);
    throw(&code, &msg);
}
