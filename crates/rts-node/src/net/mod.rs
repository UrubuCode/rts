//! `node:net` — TCP `Socket`/`Server`, `BlockList`, `SocketAddress`,
//! `isIP`/`isIPv4`/`isIPv6`, against `docs/reference/node/net.md`.
//!
//! # Reuse-check, before anything here was written
//!
//! `.claude/skills/reuse-check/SKILL.md`'s search turned up nothing this
//! module could call instead of writing: `rts-cranelift` has no socket, no
//! background-thread ↔ JS handoff, and no async primitive (the search table
//! there is entirely about value encodings, shapes and ABI — none of which a
//! network module touches). Inside this crate, the answer is not "nothing"
//! but "half of it, and reuse it rather than re-derive it": `fs/watch.rs`
//! already solved "a background thread's events reach JS without the thread
//! ever calling in" — this module's [`registry`] is that same recipe, only
//! generalised to two tables sharing one pump instead of one. And
//! `stream/mod.rs` already IS the Duplex a `Socket` needs to be — see
//! `socket.rs`'s own doc for exactly how much of it this module reuses
//! versus is forced to duplicate by module privacy.
//!
//! # Why blocking `std::net`, not tokio
//!
//! `docs/reference/node/net.md` §5.1 specifies `tokio`/`socket2`. Neither is
//! a dependency of this crate (`Cargo.toml`'s manifest, which this module
//! must not edit — five other agents are editing sibling modules under this
//! same file lock right now), and the shared crate list backing that
//! manifest's own comment does not carry them either. So this module is
//! built on `std::net`'s blocking primitives plus a background thread per
//! socket/server, following [`registry`]'s doc — a real divergence from the
//! reference doc's own plan, named here rather than silently substituted.
//!
//! # WHEN a `'data'`/`'connection'` listener runs
//!
//! See `socket.rs`'s and `server.rs`'s own doc sections (their answers
//! differ in the source thread, not in the mechanism): a queued event's
//! listener runs synchronously, just before the NEXT call this module's own
//! natives make into [`registry::pump`]. A program that calls `listen()` (or
//! `connect()`) and then makes no further `node:net` call never observes its
//! `'connection'`/`'data'`/`'connect'` listener fire — there is no timer, no
//! event loop, and no other point this engine hands control back to that
//! this crate can hook. That is the same real, named limit `fs/watch.rs`
//! states for the same reason, not the worst of the choices available: a
//! program that already talks to the socket/server again (write, then read
//! the reply; accept, then write to the accepted socket — the overwhelmingly
//! common shape) observes its listeners.
//!
//! # Not implemented, by name
//!
//! **IPC** (Unix-domain sockets, Windows named pipes) — every `path`-based
//! overload of `connect`/`createConnection`/`server.listen` is refused, TCP-only
//! throughout. Refused OUT LOUD now (`ERR_INVALID_ARG_VALUE`, `validate`'s own
//! doc explains the shape): it used to be silent, this crate's convention for a
//! gap it could not report, and a silently ignored `path` connects somewhere
//! else or nowhere at all. **Happy
//! Eyeballs / `autoSelectFamily`** and its four getter/setter pairs
//! (`getDefaultAutoSelectFamily`, …) — needs `node:dns`'s `lookup()`, a
//! cross-module dependency out of scope here. `fd`/`onread`/`signal` socket
//! options. `socket.destroySoon`, `resetAndDestroy`,
//! `getTypeOfService`/`setTypeOfService`. `'connectionAttempt*'` events (only
//! meaningful under Happy Eyeballs). `'drop'` and `maxConnections`/
//! `dropMaxConnection` enforcement — the properties exist and are settable,
//! nothing reads them. `'lookup'` event. `socket.bufferSize` (deprecated
//! alias; use `writable.writableLength`, which this module's inherited
//! `Writable` half already tracks correctly). `server.listen(handle)` — the
//! `(port[, host][, callback])` and `(options[, callback])` forms are both read
//! now, and both refuse a bad port by Node's own code (see `validate`).
//! `server[Symbol.
//! asyncDispose]`. `reusePort`/`ipv6Only`/`exclusive`/`backlog` listen
//! options — `std::net::TcpListener::bind` takes none of them.
//! `setKeepAlive`'s OS-level effect — see `socket.rs`'s own note; the value
//! is recorded, never applied. `server.getConnections`'s count — always `0`;
//! see `server.rs`'s own note.

mod blocklist;
mod common;
mod ip;
mod registry;
mod server;
mod socket;
mod socket_address;
mod validate;

use rts_core::entry::{self, Provided};

/// This module as a loop source; see the function it re-exports.
pub use registry::source;

/// The namespace `node:net` is.
pub fn namespace(context: &mut entry::Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("createConnection", create_connection),
        ("connect", create_connection),
        ("createServer", create_server),
        ("isIP", ip::is_ip),
        ("isIPv4", ip::is_ipv4),
        ("isIPv6", ip::is_ipv6),
        ("getDefaultAutoSelectFamily", get_default_auto_select_family),
        ("setDefaultAutoSelectFamily", set_default_auto_select_family),
        ("getDefaultAutoSelectFamilyAttemptTimeout", get_default_auto_select_family_timeout),
        ("setDefaultAutoSelectFamilyAttemptTimeout", set_default_auto_select_family_timeout),
    ];
    let namespace = entry::make_namespace(context, members);
    // `net.SOMAXCONN` — the default backlog Node's own `lib/net.js` falls
    // back to when `listen()` is not given one. Node reads this from libuv's
    // `SOMAXCONN`, which is platform-defined (511 on the platforms Node ships
    // for; some historically use 128). Either is "a positive backlog max",
    // which is all a portable value here can promise without asking the OS.
    let somaxconn = entry::make_number(511.0);
    entry::put_member(context, namespace, "SOMAXCONN", somaxconn);

    let socket_ctor = class_ctor(context, socket::construct, socket::prototype);
    let server_ctor = class_ctor(context, server::construct, server::prototype);
    let blocklist_ctor = class_ctor(context, blocklist::construct, blocklist::prototype);
    let is_block_list = entry::make_callable(context, blocklist::is_block_list);
    entry::put_member(context, blocklist_ctor, "isBlockList", is_block_list);
    let address_ctor = class_ctor(context, socket_address::construct, socket_address::prototype);
    let parse_fn = entry::make_callable(context, socket_address::parse);
    entry::put_member(context, address_ctor, "parse", parse_fn);

    entry::put_member(context, namespace, "Socket", socket_ctor);
    entry::put_member(context, namespace, "Server", server_ctor);
    entry::put_member(context, namespace, "BlockList", blocklist_ctor);
    entry::put_member(context, namespace, "SocketAddress", address_ctor);
    namespace
}

fn class_ctor(context: &mut entry::Context, construct: Provided, prototype_of: fn(&mut entry::Context) -> u64) -> u64 {
    let ctor = entry::make_callable(context, construct);
    let prototype = prototype_of(context);
    entry::put_member(context, ctor, "prototype", prototype);
    ctor
}

/// `net.createConnection(...)` / `net.connect(...)` — builds a `Socket` and
/// immediately calls `.connect()` on it with the same arguments, matching
/// real Node's documented relationship between the two (`connect` is a pure
/// alias). Only the `(port[, host][, listener])` and `(options[,
/// listener])` forms reach `Socket::connect`; see the module doc for the
/// path form.
extern "C" fn create_connection(e: u64, _this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    let socket = socket::construct(e, 0, 0, 0, 0, 0);
    socket::connect(e, socket, a, b, c, d);
    socket
}

/// `net.createServer([options][, connectionListener])`.
extern "C" fn create_server(e: u64, _this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    server::construct(e, 0, a, b, c, d)
}

/// Process-wide `autoSelectFamily` state — Happy Eyeballs itself is not
/// implemented (see the module doc), but the on/off FLAG and the attempt-
/// timeout NUMBER are just settings a program round-trips through these four
/// functions, with no socket behavior behind them to build. Node's own
/// default is `true` (since 18.13/20 made it the default); this used to
/// answer a hardcoded `false`, which is not Node's default and, worse, could
/// never be changed — `setDefaultAutoSelectFamily` was a no-op, so a program
/// that turned it off and read it back still saw the flag it just cleared.
static AUTO_SELECT_FAMILY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
/// Node's own default (`lib/net.js`), and the floor `setDefaultAutoSelectFamilyAttemptTimeout` clamps up to.
const AUTO_SELECT_FAMILY_TIMEOUT_DEFAULT: u32 = 250;
const AUTO_SELECT_FAMILY_TIMEOUT_FLOOR: u32 = 10;
static AUTO_SELECT_FAMILY_TIMEOUT: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(AUTO_SELECT_FAMILY_TIMEOUT_DEFAULT);

extern "C" fn get_default_auto_select_family(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::boolean_value(AUTO_SELECT_FAMILY.load(std::sync::atomic::Ordering::SeqCst))
}

extern "C" fn set_default_auto_select_family(_e: u64, _this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    AUTO_SELECT_FAMILY.store(entry::to_boolean(value), std::sync::atomic::Ordering::SeqCst);
    entry::undefined_value()
}

extern "C" fn get_default_auto_select_family_timeout(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::make_number(AUTO_SELECT_FAMILY_TIMEOUT.load(std::sync::atomic::Ordering::SeqCst) as f64)
}

/// Clamps up to `AUTO_SELECT_FAMILY_TIMEOUT_FLOOR` — Node's own
/// `validateInt32` + explicit floor in `lib/net.js`, which this used to skip
/// entirely (the setter was a no-op, so no value, in or out of range, ever
/// took effect).
extern "C" fn set_default_auto_select_family_timeout(_e: u64, _this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let millis = entry::number_of(value).unwrap_or(AUTO_SELECT_FAMILY_TIMEOUT_DEFAULT as f64).max(0.0) as u32;
    AUTO_SELECT_FAMILY_TIMEOUT.store(millis.max(AUTO_SELECT_FAMILY_TIMEOUT_FLOOR), std::sync::atomic::Ordering::SeqCst);
    entry::undefined_value()
}

