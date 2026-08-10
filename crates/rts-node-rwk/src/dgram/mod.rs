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
//! # Not implemented, by name
//!
//! **`connect`/`disconnect`/`remoteAddress`** — `connect()` narrows a UDP
//! socket to one peer; `std::net::UdpSocket::connect` supports it and this
//! module wires it, so `remoteAddress()` reads `peer_addr()` — both are
//! implemented, named here because the doc lists them as a pair with
//! `send()`'s connected-socket overload, which is NOT implemented: `send`
//! here always takes an explicit port/address. **`setMulticastInterface`** —
//! `std::net::UdpSocket` has no interface-by-name/index setter, and adding
//! one needs `libc`/`windows-sys`, outside this crate's current dependency
//! set. **`addSourceSpecificMembership`/`dropSourceSpecificMembership`** —
//! source-specific (IGMPv3/MLDv2) multicast has no `std` equivalent, same
//! reason. **`setMulticastTTL`** on a `udp6` socket — `std` has no
//! `IPV6_MULTICAST_HOPS` setter (only `set_multicast_ttl_v4` exists); a
//! `udp4` socket's call is implemented. **`get/setSendBufferSize`,
//! `get/setRecvBufferSize`, `getSendQueueSize`, `getSendQueueCount`** — no
//! `std` accessor for socket buffer sizes, and the queue counters are a
//! libuv-internal concept with no OS source of truth here. **`ref`/`unref`**
//! — no event-loop keep-alive accounting exists to hook; both are accepted
//! and return `this` for chaining, with no OS or scheduling effect.
//! **`bind`'s `fd`/`exclusive` options, the `lookup`/`signal`
//! `createSocket` options, `receiveBlockList`/`sendBlockList`,
//! `[Symbol.asyncDispose]`** — refused by silently answering `undefined`
//! for the option, this crate's convention; none is recorded and later
//! consulted. Each of the above answers `undefined` from its member lookup
//! (unregistered) or is a documented no-op, never a plausible wrong value.

mod registry;

use rts_core_rwk::entry::{self, Context, Provided};
use std::net::UdpSocket;

use registry::{DgramEvent, SocketEntry};

const METHODS: &[(&str, Provided)] = &[
    ("bind", bind),
    ("connect", connect),
    ("disconnect", disconnect),
    ("close", close),
    ("send", send),
    ("address", address),
    ("remoteAddress", remote_address),
    ("setBroadcast", set_broadcast),
    ("setTTL", set_ttl),
    ("setMulticastTTL", set_multicast_ttl),
    ("setMulticastLoopback", set_multicast_loopback),
    ("addMembership", add_membership),
    ("dropMembership", drop_membership),
    ("ref", ref_unref),
    ("unref", ref_unref),
];

/// This module as a loop source; see the function it re-exports.
pub use registry::source;

/// The namespace `node:dgram` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[("createSocket", create_socket)];
    let namespace = entry::make_namespace(context, members);
    let ctor = entry::make_callable(context, construct);
    let prototype = prototype(context);
    entry::put_member(context, ctor, "prototype", prototype);
    entry::put_member(context, namespace, "Socket", ctor);
    namespace
}

fn prototype(context: &mut Context) -> u64 {
    let parent = entry::make_prototype(context, "EventEmitter", &[]);
    // Registered as `"dgram.Socket"`, not the bare `"Socket"`: `net::socket`
    // registers its OWN, differently-shaped TCP `Socket` prototype under that
    // same bare name, and `make_prototype` is idempotent BY NAME — whichever
    // module's `namespace()` ran first won it. Install order puts `dgram`
    // before `net`, so every `net.Socket` instance was getting `dgram`'s UDP
    // method table (`bind`/`send`/…, no `connect`/`setTimeout`/`setNoDelay`),
    // which read as `sock.setTimeout is not a function`. The property name
    // programs see (`dgram.Socket`, `new dgram.Socket()`... actually just
    // `Socket` off the `dgram` namespace object) is unaffected — that comes
    // from the `put_member(namespace, "Socket", ctor)` below, a different key
    // space from this prototype registry.
    let made = entry::make_prototype(context, "dgram.Socket", METHODS);
    entry::set_prototype_in(context, made, parent);
    made
}

/// `dgram.createSocket(type | options, callback?)`.
extern "C" fn create_socket(e: u64, _this: u64, a: u64, callback: u64, _c: u64, _d: u64) -> u64 {
    let socket = construct(e, 0, a, 0, 0, 0);
    let absent = entry::undefined_value();
    if callback != absent {
        let on_fn = entry::with_runtime(|context| entry::get_member(context, socket, "on"));
        if on_fn != absent {
            let event = key("message");
            entry::call(on_fn, socket, event, callback, absent, absent);
        }
    }
    socket
}

/// `new dgram.Socket(type | options)` — not part of Node's public API
/// (`createSocket` is the only constructor a program should reach), kept as
/// the shared build [`create_socket`] and [`namespace`]'s registered `Socket`
/// both call, following the pattern every other class in this crate uses.
extern "C" fn construct(_e: u64, this: u64, options: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    registry::pump();
    let (kind, reuse_raw) = entry::with_runtime(|context| {
        let text = entry::text_in(context, options);
        match text {
            Some(kind) => (kind, entry::undefined_in(context)),
            None => {
                let kind = option_text(context, options, "type").unwrap_or_else(|| "udp4".to_owned());
                let reuse = option_value(context, options, "reuseAddr");
                (kind, reuse)
            }
        }
    });
    // Decoded OUTSIDE the borrow above: `to_boolean` is an ambient entry point
    // (it calls `with_current` itself), so decoding it while still inside
    // `with_runtime`'s closure would be the nested borrow this crate aborts on.
    let reuse_addr = entry::to_boolean(reuse_raw);
    let is_udp6 = kind == "udp6";
    entry::with_runtime(|context| {
        let prototype = prototype(context);
        let instance = self_or_new(context, this, prototype);
        init_emitter(context, instance);
        set_bool(context, instance, "__udp6", is_udp6);
        set_bool(context, instance, "__reuseAddr", reuse_addr);
        set_bool(context, instance, "__bound", false);
        instance
    })
}

/// `socket.bind(port?, address?, callback?)` / `bind(options, callback?)`.
///
/// Binding here is synchronous at the syscall level — only `'listening'`
/// itself is queued through [`registry::pump`], the same divergence
/// `net.md`'s own "landed" section names for TCP's `connect`/`accept`.
extern "C" fn bind(_e: u64, this: u64, a: u64, b: u64, c: u64, _d: u64) -> u64 {
    registry::pump();
    let absent = entry::undefined_value();
    let (port, address, callback) = entry::with_runtime(|context| match entry::number_of(a) {
        // `string_in` and not `text_in`: the second is `ToString`, so an ABSENT
        // address arrived as the literal host name `"undefined"`, went to the
        // resolver, and came back as WSAHOST_NOT_FOUND — an `'error'` event
        // nothing handled, which ended the program. `bind(0)` is the common
        // spelling and it could not work.
        //
        // `modules::string_in` says so in its own doc: a coercion that can be
        // mistaken for a test will be.
        Some(port) => (
            port as u16,
            entry::string_in(context, b).unwrap_or_default(),
            c,
        ),
        None => {
            let port = option_num(context, a, "port").unwrap_or(0.0) as u16;
            let address = option_text(context, a, "address").unwrap_or_default();
            (port, address, b)
        }
    });
    let is_udp6 = get_bool(this, "__udp6");
    let bind_addr = if address.is_empty() {
        if is_udp6 { format!("[::]:{port}") } else { format!("0.0.0.0:{port}") }
    } else if is_udp6 {
        format!("[{address}]:{port}")
    } else {
        format!("{address}:{port}")
    };
    if callback != absent {
        once(this, "listening", callback);
    }
    match UdpSocket::bind(&bind_addr) {
        Ok(socket) => {
            let id = registry::next_id();
            let reader = socket.try_clone().ok();
            entry::with_runtime(|context| {
                set_num(context, this, "__socketId", id as f64);
                set_bool(context, this, "__bound", true);
            });
            registry::with_sockets(|table| {
                table.insert(
                    id,
                    SocketEntry { owner: std::thread::current().id(), instance: this, queue: Default::default(), socket: Some(socket), closed: false },
                );
                if let Some(entry) = table.get_mut(&id) {
                    entry.queue.push_back(DgramEvent::Listening);
                }
            });
            if let Some(reader) = reader {
                registry::spawn_reader(id, reader);
            }
        }
        Err(error) => {
            let id = registry::next_id();
            registry::with_sockets(|table| {
                table.insert(id, SocketEntry { owner: std::thread::current().id(), instance: this, queue: Default::default(), socket: None, closed: false });
                if let Some(entry) = table.get_mut(&id) {
                    entry.queue.push_back(DgramEvent::BindFailed(error.to_string()));
                }
            });
            entry::with_runtime(|context| set_num(context, this, "__socketId", id as f64));
        }
    }
    absent
}

/// `socket.connect(port, address?, callback?)` — narrows the socket to one
/// peer via `UdpSocket::connect`; unlike TCP this is a local-only OS call, no
/// background thread involved, so it completes before returning and needs no
/// registry entry of its own.
extern "C" fn connect(_e: u64, this: u64, port: u64, address: u64, callback: u64, _d: u64) -> u64 {
    registry::pump();
    let absent = entry::undefined_value();
    if !get_bool(this, "__bound") {
        bind(0, this, entry::make_number(0.0), absent, absent, 0);
    }
    let Some(id) = socket_id(this) else { return absent };
    let port = entry::number_of(port).unwrap_or(0.0) as u16;
    let host = entry::text_of(address).unwrap_or_else(|| "127.0.0.1".to_owned());
    let result = registry::with_sockets(|table| match table.get(&id).and_then(|entry| entry.socket.as_ref()) {
        Some(socket) => socket.connect((host.as_str(), port)).map_err(|error| error.to_string()),
        None => Err("socket not bound".to_owned()),
    });
    match result {
        Ok(()) => {
            if callback != absent {
                entry::call(callback, absent, absent, absent, absent, absent);
            }
            emit(this, "connect", absent, absent, absent);
        }
        Err(message) => {
            let error = entry::with_runtime(|context| {
                let object = entry::make_object(context);
                let message_v = entry::make_string(context, &message);
                entry::put_member(context, object, "message", message_v);
                object
            });
            if callback != absent {
                entry::call(callback, absent, error, absent, absent, absent);
            } else {
                emit(this, "error", error, absent, absent);
            }
        }
    }
    absent
}

/// `socket.disconnect()` — the inverse of [`connect`]; `UdpSocket` has no
/// "unconnect" call, so this reconnects to the OS-assigned wildcard, matching
/// what an unconnected socket accepts datagrams from.
extern "C" fn disconnect(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let Some(id) = socket_id(this) else { return absent };
    registry::with_sockets(|table| {
        if let Some(entry) = table.get(&id)
            && let Some(socket) = &entry.socket
        {
            let wildcard = if get_bool(this, "__udp6") { "[::]:0" } else { "0.0.0.0:0" };
            let _ = socket.connect(wildcard);
        }
    });
    absent
}

/// `socket.close(callback?)`.
extern "C" fn close(_e: u64, this: u64, callback: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    if callback != absent {
        once(this, "close", callback);
    }
    let Some(id) = socket_id(this) else { return absent };
    registry::with_sockets(|table| {
        if let Some(entry) = table.get_mut(&id) {
            entry.closed = true;
            entry.queue.push_back(DgramEvent::Closed);
        }
    });
    registry::pump();
    absent
}

/// `socket.send(msg, port?, address?, callback?)` — the offset/length overload
/// (buffer-only) is not read; see the module doc.
extern "C" fn send(_e: u64, this: u64, msg: u64, port: u64, address: u64, callback: u64) -> u64 {
    registry::pump();
    let absent = entry::undefined_value();
    if !get_bool(this, "__bound") {
        bind(0, this, entry::make_number(0.0), absent, absent, 0);
    }
    let bytes = entry::text_of(msg)
        .and_then(|text| entry::encode_text(&text, "utf8"))
        .or_else(|| entry::with_runtime(|context| entry::bytes_of(context, msg)))
        .unwrap_or_default();
    let port_num = entry::number_of(port);
    let target_host = entry::text_of(address);
    let Some(id) = socket_id(this) else { return absent };
    let result = registry::with_sockets(|table| {
        let Some(entry) = table.get(&id) else { return Err("socket closed".to_owned()) };
        let Some(socket) = &entry.socket else { return Err("socket not bound".to_owned()) };
        match port_num {
            Some(port) => {
                let host = target_host.unwrap_or_else(|| if get_bool_static(&entry.instance) { "::1".to_owned() } else { "127.0.0.1".to_owned() });
                socket.send_to(&bytes, (host.as_str(), port as u16)).map(|_| ()).map_err(|error| error.to_string())
            }
            // No port: the socket must already be connected via `connect()`.
            None => socket.send(&bytes).map(|_| ()).map_err(|error| error.to_string()),
        }
    });
    match result {
        Ok(()) if callback != absent => entry::call(callback, absent, absent, absent, absent, absent),
        Ok(()) => absent,
        Err(message) => {
            let error = entry::with_runtime(|context| {
                let object = entry::make_object(context);
                let message_v = entry::make_string(context, &message);
                entry::put_member(context, object, "message", message_v);
                object
            });
            if callback != absent {
                entry::call(callback, absent, error, absent, absent, absent)
            } else {
                emit(this, "error", error, absent, absent);
                absent
            }
        }
    }
}

// `send`'s host default branches on address family; the socket entry itself
// carries no such flag (only the JS instance does), so this reads it off the
// JS instance passed in — a plain member read, not an ambient-context call.
fn get_bool_static(instance: &u64) -> bool {
    get_bool(*instance, "__udp6")
}

/// `socket.address()`.
extern "C" fn address(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(id) = socket_id(this) else { return entry::undefined_value() };
    let local = registry::with_sockets(|table| {
        table.get(&id).and_then(|entry| entry.socket.as_ref()).and_then(|socket| socket.local_addr().ok())
    });
    match local {
        Some(addr) => address_info(addr.ip().to_string(), addr.port(), get_bool(this, "__udp6")),
        None => entry::undefined_value(),
    }
}

/// `socket.remoteAddress()` — see the module doc: implemented via
/// `UdpSocket::peer_addr`, valid only after [`connect`].
extern "C" fn remote_address(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(id) = socket_id(this) else { return entry::undefined_value() };
    let peer = registry::with_sockets(|table| {
        table.get(&id).and_then(|entry| entry.socket.as_ref()).and_then(|socket| socket.peer_addr().ok())
    });
    match peer {
        Some(addr) => address_info(addr.ip().to_string(), addr.port(), get_bool(this, "__udp6")),
        None => entry::undefined_value(),
    }
}

fn address_info(ip: String, port: u16, is_udp6: bool) -> u64 {
    entry::with_runtime(|context| {
        let object = entry::make_object(context);
        let addr_v = entry::make_string(context, &ip);
        let family_v = entry::make_string(context, if is_udp6 { "IPv6" } else { "IPv4" });
        let port_v = entry::make_number(f64::from(port));
        entry::put_member(context, object, "address", addr_v);
        entry::put_member(context, object, "family", family_v);
        entry::put_member(context, object, "port", port_v);
        object
    })
}

/// `socket.setBroadcast(flag)`.
extern "C" fn set_broadcast(_e: u64, this: u64, flag: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    with_socket(this, |socket| socket.set_broadcast(entry::to_boolean(flag)));
    entry::undefined_value()
}

/// `socket.setTTL(ttl)`.
extern "C" fn set_ttl(_e: u64, this: u64, ttl: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let ttl = entry::number_of(ttl).unwrap_or(64.0) as u32;
    with_socket(this, |socket| socket.set_ttl(ttl));
    entry::undefined_value()
}

/// `socket.setMulticastTTL(ttl)` — `udp4` only, see the module doc.
extern "C" fn set_multicast_ttl(_e: u64, this: u64, ttl: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if get_bool(this, "__udp6") {
        return entry::undefined_value();
    }
    let ttl = entry::number_of(ttl).unwrap_or(1.0) as u32;
    with_socket(this, |socket| socket.set_multicast_ttl_v4(ttl));
    entry::undefined_value()
}

/// `socket.setMulticastLoopback(flag)`.
extern "C" fn set_multicast_loopback(_e: u64, this: u64, flag: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let flag = entry::to_boolean(flag);
    let is_udp6 = get_bool(this, "__udp6");
    with_socket(this, |socket| if is_udp6 { socket.set_multicast_loop_v6(flag) } else { socket.set_multicast_loop_v4(flag) });
    entry::undefined_value()
}

/// `socket.addMembership(multicastAddress, multicastInterface?)` — the
/// interface argument is read only far enough to pick `INADDR_ANY`/`::` when
/// absent; a specific interface address IS honoured (`std` takes one), a
/// by-name/by-index interface is not (see the module doc).
extern "C" fn add_membership(_e: u64, this: u64, group: u64, iface: u64, _c: u64, _d: u64) -> u64 {
    membership(this, group, iface, true);
    entry::undefined_value()
}

/// `socket.dropMembership(multicastAddress, multicastInterface?)`.
extern "C" fn drop_membership(_e: u64, this: u64, group: u64, iface: u64, _c: u64, _d: u64) -> u64 {
    membership(this, group, iface, false);
    entry::undefined_value()
}

fn membership(this: u64, group: u64, iface: u64, join: bool) {
    let Some(group) = entry::text_of(group) else { return };
    let is_udp6 = get_bool(this, "__udp6");
    with_socket(this, |socket| {
        if is_udp6 {
            let Ok(group) = group.parse::<std::net::Ipv6Addr>() else { return Ok(()) };
            if join { socket.join_multicast_v6(&group, 0) } else { socket.leave_multicast_v6(&group, 0) }
        } else {
            let Ok(group) = group.parse::<std::net::Ipv4Addr>() else { return Ok(()) };
            let interface = entry::text_of(iface).and_then(|text| text.parse().ok()).unwrap_or(std::net::Ipv4Addr::UNSPECIFIED);
            if join { socket.join_multicast_v4(&group, &interface) } else { socket.leave_multicast_v4(&group, &interface) }
        }
    });
}

/// `socket.ref()`/`socket.unref()` — see the module doc: recorded nowhere,
/// no event-loop keep-alive to affect; chainable, matching Node's return
/// type.
extern "C" fn ref_unref(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    this
}

fn with_socket(this: u64, body: impl FnOnce(&UdpSocket) -> std::io::Result<()>) {
    let Some(id) = socket_id(this) else { return };
    registry::with_sockets(|table| {
        if let Some(entry) = table.get(&id)
            && let Some(socket) = &entry.socket
        {
            let _ = body(socket);
        }
    });
}

fn socket_id(this: u64) -> Option<u64> {
    entry::number_of(get_value(this, "__socketId")).map(|value| value as u64)
}

/// Calls `this.emit(event, a0, a1, a2)` — looked up fresh, never while a
/// runtime borrow is held, the same recipe `net/common.rs::emit` uses and for
/// the same reason.
pub(super) fn emit(this: u64, event: &str, a0: u64, a1: u64, a2: u64) {
    let emit_fn = entry::with_runtime(|context| entry::get_member(context, this, "emit"));
    let absent = entry::undefined_value();
    if emit_fn == absent {
        return;
    }
    let event_key = key(event);
    entry::call(emit_fn, this, event_key, a0, a1, a2);
}

fn once(this: u64, event: &str, listener: u64) {
    let once_fn = entry::with_runtime(|context| entry::get_member(context, this, "once"));
    let absent = entry::undefined_value();
    if once_fn != absent {
        let event = key(event);
        entry::call(once_fn, this, event, listener, absent, absent);
    }
}

fn key(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

fn get_value(this: u64, name: &str) -> u64 {
    entry::get_indexed(this, key(name))
}

fn get_bool(this: u64, name: &str) -> bool {
    entry::to_boolean(get_value(this, name))
}

fn set_bool(context: &mut Context, this: u64, name: &str, value: bool) {
    let held = entry::boolean_value(value);
    entry::put_member(context, this, name, held);
}

fn set_num(context: &mut Context, this: u64, name: &str, value: f64) {
    let held = entry::make_number(value);
    entry::put_member(context, this, name, held);
}

fn init_emitter(context: &mut Context, instance: u64) {
    let events = entry::make_object(context);
    entry::put_member(context, instance, "__events__", events);
}

fn self_or_new(context: &mut Context, this: u64, prototype: u64) -> u64 {
    match entry::is_object(context, this) {
        true => this,
        false => entry::make_instance(context, prototype),
    }
}

/// Reads one member from an options object, from a context already in
/// hand — `entry::get_member` (context-taking), never `get_indexed` (the
/// ambient form, which is a nested borrow and therefore an abort when called,
/// as every option read here is, from inside [`entry::with_runtime`]).
fn option_value(context: &mut Context, options: u64, name: &str) -> u64 {
    let absent = entry::undefined_in(context);
    if options == absent { absent } else { entry::get_member(context, options, name) }
}

fn option_text(context: &mut Context, options: u64, name: &str) -> Option<String> {
    let value = option_value(context, options, name);
    // `string_in`, which TESTS, rather than `text_in`, which converts: an option
    // the caller left out is `undefined`, and converting that answers the literal
    // text "undefined". That is what sent `bind(0)` to the resolver looking for a
    // host by that name.
    entry::string_in(context, value)
}

fn option_num(context: &mut Context, options: u64, name: &str) -> Option<f64> {
    entry::number_of(option_value(context, options, name))
}
