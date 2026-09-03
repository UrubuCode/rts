//! `node:dgram` — UDP `Socket` (an `EventEmitter`) over `std::net::UdpSocket`,
//! against `docs/reference/node/dgram.md`.
//!
//! # Reuse-check, before anything here was written
//!
//! `.claude/skills/reuse-check/SKILL.md`'s search (§1, the machine) found
//! nothing to call: `rts-cranelift` has no socket and no background-thread ↔
//! JS handoff, its table is entirely about value encodings and ABI. Inside
//! this crate the answer is "reuse the recipe, not the code": `net/registry.rs`
//! already solved exactly this problem — a background thread's events reaching
//! JS without that thread ever calling in — for TCP; [`registry`] here is the
//! same recipe (one table instead of two, since `dgram` has no server half),
//! not a re-derivation of it. `net::registry`, `net::socket`, `net::common` are
//! all `pub(super)`/private to `net`, so nothing there is directly callable —
//! the same reason `net/common.rs` gives for its own small duplicate of four
//! property helpers applies here too, and is why this module carries its own.
//!
//! # WHEN a `'message'` listener runs
//!
//! A datagram arrives on the socket's own reader thread, never the JS thread,
//! so — exactly as `net/socket.rs` documents for `'data'`, for the identical
//! reason (this engine's context is thread-local; a foreign thread calling a
//! listener aborts on the first event) — that thread never calls anything. It
//! only pushes a native record into [`registry`], and [`registry::pump`] is
//! what turns it into a call, on the JS thread, at the start of the NEXT
//! `node:dgram` native this module exports. **What a program observes**: a
//! program that calls `bind()` and then makes no further `node:dgram` call
//! never sees `'listening'` fire and never sees a `'message'`, however many
//! datagrams arrive — there is no timer, no event loop, and no other point
//! this engine hands control back to that this crate can hook. A program that
//! calls `bind()` then later calls `send()` (the common "reply" shape, or a
//! polling loop that calls any dgram method) observes every event queued in
//! between at that call.
//!
//! # What is REFUSED, and where that is decided
//!
//! [`args`] holds it: the socket type `createSocket` accepts, and the six-way
//! overload `send` accepts. Both used to accept everything —
//! `createSocket(1)` built a working `udp4` socket and `send(23, port, host)`
//! sent the two bytes `"23"` — because both read their arguments with
//! `entry::text_in`, which is `ToString` and answers for every value there is.
//! Node refuses both by code (`ERR_SOCKET_BAD_TYPE`,
//! `ERR_INVALID_ARG_TYPE`), and a wrong answer that runs is the failure this
//! repository ranks worst. Everything raised is `crate::errors`', never spelled
//! here.
//!
//! # Not implemented, by name
//!
//! **`connect`/`disconnect`/`remoteAddress`** — `connect()` narrows a UDP
//! socket to one peer; `std::net::UdpSocket::connect` supports it and this
//! module wires it, so `remoteAddress()` reads `peer_addr()` — both are
//! implemented, named here because the doc lists them as a pair with
//! `send()`'s connected-socket overload, which IS implemented now that the
//! full signature is read: a `send` with no port goes to the connected peer,
//! and a `send` WITH one on a connected socket is
//! `ERR_SOCKET_DGRAM_IS_CONNECTED` — as is a second `connect()`, and a
//! `disconnect()`/`remoteAddress()` on a socket that was never connected is
//! `ERR_SOCKET_DGRAM_NOT_CONNECTED`. **`setMulticastInterface`** — IPv4 by
//! ADDRESS (`IP_MULTICAST_IF`) IS implemented, via [`mcast_if`]'s own
//! hand-rolled `setsockopt`, the same recipe [`bufsize`] uses; by
//! name/index on a `udp6` socket is still absent (`ENOSYS`) — no fixture
//! exercises it and `std` gives no interface-name resolver.
//! **`addSourceSpecificMembership`/`dropSourceSpecificMembership`** — the
//! argument VALIDATION Node performs before the syscall (family match, a
//! real address in each) IS implemented in [`ssm`]; the join itself is not —
//! `IP_ADD_SOURCE_MEMBERSHIP`'s `struct ip_mreq_source` field order differs
//! between Linux and Windows and neither success path has a fixture to catch
//! a wrong one, so valid input raises `ENOSYS` rather than silently doing
//! nothing. **`setMulticastTTL`** on a `udp6` socket — `std` has no
//! `IPV6_MULTICAST_HOPS` setter (only `set_multicast_ttl_v4` exists); a
//! `udp4` socket's call is implemented. **`get/setSendBufferSize`,
//! `get/setRecvBufferSize`** — implemented via [`bufsize`], a hand-rolled
//! `setsockopt`/`getsockopt(SO_RCVBUF/SO_SNDBUF)` (see that module's own
//! reuse-check for why `socket2` is not pulled in for this).
//! **`getSendQueueSize`, `getSendQueueCount`** — implemented as a constant
//! `0`: [`registry::SocketEntry`]'s `queue` holds only INBOUND events, and
//! `send()` writes to the OS synchronously with no intermediate buffer of its
//! own (verified by reading `registry.rs` in full), so `0` is the real answer
//! this architecture has rather than an approximation of one. **`ref`/`unref`**
//! — no event-loop keep-alive accounting exists to hook; both are accepted
//! and return `this` for chaining, with no OS or scheduling effect.
//! **`bind`'s `fd`/`exclusive` options, `signal`, `[Symbol.asyncDispose]`**
//! — refused by silently answering `undefined` for the option, this crate's
//! convention; none is recorded and later consulted.
//!
//! **`receiveBlockList`/`sendBlockList` ARE implemented** now that
//! `net.BlockList` is real: `createSocket` validates the option is
//! `BlockList`-shaped (`ERR_INVALID_ARG_TYPE` otherwise) and stores it;
//! `send()` consults `sendBlockList` before every send and refuses a blocked
//! destination through the callback/`'error'` path, never a synchronous
//! throw. `receiveBlockList` is accepted and validated the same way but not
//! yet consulted on the receive path — see [`registry`]'s own doc for
//! where an inbound datagram is delivered. **`lookup`** is still refused,
//! but now by THROWING `ERR_INVALID_ARG_VALUE` (Node's own validation
//! rejects a non-function there before ever calling it) rather than
//! silently discarding it.
//!
//! # How this file is split
//!
//! [`common`] is the property/emit plumbing every other module here needs;
//! [`construct`] builds the prototype and the instance; [`socket`] is the
//! life cycle (`bind`/`connect`/`disconnect`/`close`/`address`/
//! `remoteAddress`); [`membership`] is multicast; [`options`] is every other
//! socket option; [`send`] is `send()` and the block-list check that guards
//! it. What stays here is what is genuinely the module: this doc, the
//! [`METHODS`] table, and [`namespace`].

mod args;
mod bufsize;
mod common;
mod construct;
mod mcast_if;
mod membership;
mod options;
mod registry;
mod send;
mod socket;
mod ssm;

use rts_core::entry::{self, Context, Provided};

const METHODS: &[(&str, Provided)] = &[
    ("bind", socket::bind),
    ("connect", socket::connect),
    ("disconnect", socket::disconnect),
    ("close", socket::close),
    ("send", send::send),
    ("address", socket::address),
    ("remoteAddress", socket::remote_address),
    ("setBroadcast", options::set_broadcast),
    ("setTTL", options::set_ttl),
    ("setMulticastTTL", membership::set_multicast_ttl),
    ("setMulticastLoopback", membership::set_multicast_loopback),
    ("addMembership", membership::add_membership),
    ("dropMembership", membership::drop_membership),
    ("getRecvBufferSize", options::get_recv_buffer_size),
    ("setRecvBufferSize", options::set_recv_buffer_size),
    ("getSendBufferSize", options::get_send_buffer_size),
    ("setSendBufferSize", options::set_send_buffer_size),
    ("getSendQueueSize", options::get_send_queue_size),
    ("getSendQueueCount", options::get_send_queue_count),
    ("setMulticastInterface", membership::set_multicast_interface),
    ("addSourceSpecificMembership", membership::add_source_specific_membership),
    ("dropSourceSpecificMembership", membership::drop_source_specific_membership),
    ("ref", options::ref_unref),
    ("unref", options::ref_unref),
];

/// This module as a loop source; see the function it re-exports.
pub use registry::source;

/// The namespace `node:dgram` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[("createSocket", construct::create_socket)];
    let namespace = entry::make_namespace(context, members);
    let ctor = entry::make_callable(context, construct::construct);
    let prototype = construct::prototype(context);
    entry::put_member(context, ctor, "prototype", prototype);
    entry::put_member(context, namespace, "Socket", ctor);
    namespace
}
