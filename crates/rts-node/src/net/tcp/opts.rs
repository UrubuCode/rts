//! node:net — option-object normalization and the module's errors.
//!
//! Node's `ServerOptions`/`SocketConstructorOptions`/`ListenOptions`/
//! `SocketConnectOpts` are read here, once, so the class impls stay about
//! behaviour. Options RTS has not implemented are REFUSED (`ERR_INVALID_ARG_VALUE`)
//! rather than silently ignored — see net.md §8.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use super::state::SocketOpts;
use crate::values::{opt_bool, opt_has, opt_num, opt_str};

unsafe extern "C" {
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
    fn __rtsadp_make_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64) -> u64;
}

/// The JS Error CLASS a code is an instance of — Node's own choice per code.
/// It must be a REGISTERED class name: the engine fabricates the instance on
/// that class's shape, and an unknown name degrades to a bare string (no
/// `.message`).
fn class_of(code: &str) -> &'static str {
    match code {
        "ERR_INVALID_ARG_TYPE" | "ERR_INVALID_ARG_VALUE" | "ERR_MISSING_ARGS"
        | "ERR_INVALID_FD_TYPE" | "ERR_INVALID_ADDRESS" => "TypeError",
        "ERR_SOCKET_BAD_PORT" | "ERR_OUT_OF_RANGE" => "RangeError",
        _ => "Error",
    }
}

/// Throw a real Error-family instance. The code rides the message because the
/// engine's Error shape has no `code` slot (net.md §8).
pub fn throw(code: &str, message: &str) {
    let class = class_of(code);
    let msg = format!("{code}: {message}");
    unsafe {
        __rtsadp_throw_js_error(class.as_ptr(), class.len() as i64, msg.as_ptr(), msg.len() as i64);
    }
}

/// The same error as a VALUE (a callback's err-first argument / `'error'`).
pub fn error_value(code: &str, message: &str) -> u64 {
    let class = class_of(code);
    let msg = format!("{code}: {message}");
    unsafe { __rtsadp_make_js_error(class.as_ptr(), class.len() as i64, msg.as_ptr(), msg.len() as i64) }
}

/// The Node error code for a socket-op `io::Error` (§4's table).
pub fn code_for(e: &std::io::Error) -> &'static str {
    use std::io::ErrorKind::*;
    match e.kind() {
        AddrInUse => return "EADDRINUSE",
        AddrNotAvailable => return "EADDRNOTAVAIL",
        ConnectionRefused => return "ECONNREFUSED",
        ConnectionReset => return "ECONNRESET",
        ConnectionAborted => return "ECONNABORTED",
        PermissionDenied => return "EACCES",
        BrokenPipe => return "EPIPE",
        TimedOut => return "ETIMEDOUT",
        NotConnected => return "ENOTCONN",
        HostUnreachable => return "EHOSTUNREACH",
        NetworkUnreachable => return "ENETUNREACH",
        InvalidInput => return "EINVAL",
        _ => {}
    }
    match e.raw_os_error() {
        Some(10013) | Some(13) => "EACCES",
        Some(10048) | Some(98) => "EADDRINUSE",
        Some(10049) | Some(99) => "EADDRNOTAVAIL",
        Some(10054) | Some(104) => "ECONNRESET",
        Some(10060) | Some(110) => "ETIMEDOUT",
        Some(10061) | Some(111) => "ECONNREFUSED",
        Some(10024) | Some(24) => "EMFILE",
        Some(32) => "EPIPE",
        _ => "UNKNOWN",
    }
}

/// `(code, "<os message>, <op>")` — the shape Node prints for a failed op.
pub fn message_for(e: &std::io::Error, op: &str) -> (String, String) {
    (code_for(e).to_string(), format!("{e}, {op}"))
}

/// Node's port validation (`ERR_SOCKET_BAD_PORT`). `listen` accepts 0 (the OS
/// picks); `connect` needs a real port, which its caller checks.
pub fn port_of(n: f64) -> Result<u16, String> {
    if !n.is_finite() || n.fract() != 0.0 || !(0.0..=65535.0).contains(&n) {
        return Err(format!("Port should be >= 0 and < 65536. Received {n}."));
    }
    Ok(n as u16)
}

/// Options RTS has not implemented. Passing one throws rather than being
/// ignored — the caller asked for behaviour that would not happen.
const UNSUPPORTED: &[(&str, &str)] = &[
    ("fd", "wrapping an existing fd needs the descriptor-passing path"),
    ("onread", "the custom read-buffer option needs the stream layer"),
    ("path", "IPC (Unix-domain sockets / Windows named pipes) is not implemented"),
    ("signal", "AbortSignal wiring is not implemented — use close()/destroy()"),
    ("lookup", "a custom lookup function is not implemented — the system resolver is used"),
    ("readableAll", "IPC pipe permission bits are not implemented"),
    ("writableAll", "IPC pipe permission bits are not implemented"),
];

/// Refuse any unimplemented option present on `options`. Returns false (and
/// throws) when one is found.
pub fn reject_unsupported(options: u64) -> bool {
    for (name, why) in UNSUPPORTED {
        if opt_has(options, name) {
            throw(
                "ERR_INVALID_ARG_VALUE",
                &format!("the '{name}' option is not implemented yet in RTS — {why}"),
            );
            return false;
        }
    }
    true
}

/// Read the socket options shared by `ServerOptions` and
/// `SocketConstructorOptions`.
pub fn socket_opts(options: u64) -> SocketOpts {
    SocketOpts {
        allow_half_open: opt_bool(options, "allowHalfOpen"),
        keep_alive: opt_bool(options, "keepAlive"),
        keep_alive_initial_delay: opt_num(options, "keepAliveInitialDelay").unwrap_or(0.0).max(0.0) as u64,
        no_delay: opt_bool(options, "noDelay"),
        pause_on_connect: opt_bool(options, "pauseOnConnect"),
    }
}

/// `server.listen(options)` — the fields RTS implements.
pub struct ListenArgs {
    pub port: u16,
    pub host: Option<String>,
    pub backlog: i32,
    pub ipv6_only: bool,
    pub reuse_port: bool,
    pub exclusive: bool,
}

impl Default for ListenArgs {
    fn default() -> Self {
        Self {
            port: 0,
            host: None,
            // Node's default is 511 (not 512); the OS still caps it
            // (somaxconn/tcp_max_syn_backlog) — never silently clamped here.
            backlog: 511,
            ipv6_only: false,
            reuse_port: false,
            exclusive: false,
        }
    }
}

/// Read a `ListenOptions` object. `Err` = an error was already thrown.
pub fn listen_options(o: u64) -> Result<ListenArgs, ()> {
    if !reject_unsupported(o) {
        return Err(());
    }
    let mut a = ListenArgs::default();
    if let Some(n) = opt_num(o, "port") {
        match port_of(n) {
            Ok(p) => a.port = p,
            Err(msg) => {
                throw("ERR_SOCKET_BAD_PORT", &msg);
                return Err(());
            }
        }
    }
    a.host = opt_str(o, "host");
    if let Some(n) = opt_num(o, "backlog") {
        a.backlog = n.max(0.0) as i32;
    }
    a.ipv6_only = opt_bool(o, "ipv6Only");
    a.reuse_port = opt_bool(o, "reusePort");
    // `exclusive` only means something under node:cluster (whether workers share
    // the handle); a single-process runtime never shares one, so both values
    // already behave exactly as they do in Node outside a cluster.
    a.exclusive = opt_bool(o, "exclusive");
    Ok(a)
}

// ─── Module-level config (net.get/setDefaultAutoSelectFamily*) ───────────────
//
// Process-wide in real Node (a single global, not per-thread) — matched here as
// process-wide atomics.

/// Default `true` since Node v20.0.0/v18.18.0.
static AUTO_SELECT_FAMILY: AtomicBool = AtomicBool::new(true);
/// Default 250 ms; values < 10 clamp up to 10.
static AUTO_SELECT_FAMILY_TIMEOUT: AtomicI64 = AtomicI64::new(250);

pub fn auto_select_family() -> bool {
    AUTO_SELECT_FAMILY.load(Ordering::Acquire)
}

pub fn set_auto_select_family(v: bool) {
    AUTO_SELECT_FAMILY.store(v, Ordering::Release);
}

pub fn auto_select_family_timeout() -> i64 {
    AUTO_SELECT_FAMILY_TIMEOUT.load(Ordering::Acquire)
}

pub fn set_auto_select_family_timeout(ms: i64) {
    AUTO_SELECT_FAMILY_TIMEOUT.store(ms.max(10), Ordering::Release);
}
