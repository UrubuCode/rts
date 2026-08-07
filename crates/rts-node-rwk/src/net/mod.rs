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
//! overload of `connect`/`createConnection`/`server.listen` is refused
//! (silently, this crate's convention), TCP-only throughout. **Happy
//! Eyeballs / `autoSelectFamily`** and its four getter/setter pairs
//! (`getDefaultAutoSelectFamily`, …) — needs `node:dns`'s `lookup()`, a
//! cross-module dependency out of scope here. `fd`/`onread`/`signal` socket
//! options. `socket.destroySoon`, `resetAndDestroy`,
//! `getTypeOfService`/`setTypeOfService`. `'connectionAttempt*'` events (only
//! meaningful under Happy Eyeballs). `'drop'` and `maxConnections`/
//! `dropMaxConnection` enforcement — the properties exist and are settable,
//! nothing reads them. `'lookup'` event. `socket.bufferSize` (deprecated
//! alias; use `writable.writableLength`, which this module's inherited
//! `Writable` half already tracks correctly). `server.listen(handle)`/
//! `listen(options)` — only `(port[, host][, callback])`. `server[Symbol.
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

use rts_core_rwk::entry::{self, Provided};

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
        ("setDefaultAutoSelectFamily", no_op),
        ("getDefaultAutoSelectFamilyAttemptTimeout", get_default_auto_select_family_timeout),
        ("setDefaultAutoSelectFamilyAttemptTimeout", no_op),
    ];
    let namespace = entry::make_namespace(context, members);

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

extern "C" fn get_default_auto_select_family(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::boolean_value(false)
}

extern "C" fn get_default_auto_select_family_timeout(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::make_number(250.0)
}

/// `setDefaultAutoSelectFamily`/`setDefaultAutoSelectFamilyAttemptTimeout` —
/// accepted and discarded: Happy Eyeballs itself is not implemented (see the
/// module doc), so there is nothing for either setter to configure.
extern "C" fn no_op(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::undefined_value()
}
