//! `node:dgram` — UDP datagram sockets. Real OS sockets (`socket2` over the
//! platform's own BSD-socket API), real datagrams, real multicast. Nothing here
//! is simulated.
//!
//! Surface (Node 25 parity): `createSocket(type | options[, listener])` and the
//! `Socket` class — `bind`/`close`/`connect`/`disconnect`, `address`/
//! `remoteAddress`, `send` (every overload: `Buffer`/`TypedArray`/`DataView`/
//! `string`/array, with or without `offset`/`length`, connected or addressed),
//! the `receiveBlockList`/`sendBlockList` filters (real `net.BlockList` rules),
//! multicast `add`/`dropMembership` + `add`/`dropSourceSpecificMembership`,
//! `setTTL`/`setMulticastTTL`/`setMulticastLoopback`/`setBroadcast`/
//! `setMulticastInterface`, the buffer-size family, `getSendQueueSize`/`Count`,
//! `ref`/`unref`, and the `EventEmitter` surface it inherits in Node
//! (`on`/`once`/`off`/`emit`/…). Events: `'message'`, `'listening'`,
//! `'connect'`, `'close'`, `'error'`.
//!
//! ## How it fits the engine
//!
//! `dgram` is non-primordial — no native literal syntax, reached only through a
//! function call — so it is DATA in the Registry: `e.ns("node:dgram")` +
//! `e.class("Socket")`, resolved by the same generic path `node:fs` uses. The
//! engine never names it.
//!
//! `Socket` is an object-backed Registry class (an `Entry::Map` tagged
//! `__rts_class = "Socket"`), and that handle keys the side table in
//! [`state`] where the OS socket, listeners and pending events live.
//!
//! Node overloads by ARGUMENT TYPE at one arity (`createSocket('udp4')` vs
//! `createSocket({type:'udp4'})`; `bind(port)` vs `bind(options)` vs
//! `bind(callback)`) while the Registry resolves by ARITY. So each arity is one
//! member taking `PolyValue` words, and the impl branches on the value's tag —
//! the same decision Node's own JS makes, done polymorphically on the value
//! rather than guessed from a call shape.
//!
//! ## Async model
//!
//! Inbound datagrams come from one reader thread per bound socket ([`reader`]),
//! which only ever queues plain bytes. The event loop drains that queue on the
//! JS thread through the engine's generic pump registry
//! ([`rts_engine::loop_sources`], which knows nothing about dgram) — see
//! [`pump`]. A bound, ref'd socket keeps the loop alive, so a listening script
//! keeps running until `close()`/`unref()`. The loop's idle-window cap applies
//! (`rts-std::event_loop`): with no traffic at all it returns instead of blocking
//! forever, the same documented deviation `fs.watch` carries.
//!
//! ## Deferred (explicitly, never faked)
//!
//! - `createSocket`'s `lookup` and `signal` options, and `bind`'s `fd` option:
//!   passing one throws `ERR_INVALID_ARG_VALUE` rather than being ignored.
//!   Hostname resolution itself works — it uses the system resolver (what the
//!   default `dns.lookup` is).
//! `bind`'s `exclusive` is NOT in that list: it selects whether `node:cluster`
//! workers share the underlying handle, and a single-process runtime never
//! shares one — both values already behave here exactly as they do in Node
//! outside a cluster. `reuseAddr`/`reusePort` do the OS-level work.
//! - `[Symbol.asyncDispose]`: needs a symbol-keyed member, which the Registry's
//!   name-keyed members cannot express yet.
//!
//! Layout: `state` (socket state + queues), `create` (createSocket), `lifecycle`
//! (bind/close/connect/address/ref), `send`, `mcast` (membership + SSM),
//! `tuning` (TTL/buffers), `emitter` (the EventEmitter surface), `reader` (the
//! recv thread), `pump` (JS-thread delivery), `errors`, `mod` (registration).

mod create;
mod emitter;
mod errors;
mod lifecycle;
mod mcast;
mod pump;
mod reader;
mod send;
mod state;
mod tuning;

use rts_engine::AbiType::{self, Bool, Handle, I64, PolyValue, StrPtr, Void};
use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

#[allow(clippy::too_many_arguments)]
fn m(name: &str, kind: MemberKind, args: Vec<AbiType>, ret: AbiType, symbol: &str, ts: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig: Sig::new(args, ret),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::THROWS,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        pure: false,
        intrinsic: None,
    }
}

/// An instance method (`this` is the leading `Handle`). Every dgram method can
/// throw a Node error, so all are flagged `THROWS` — without it a builtin's
/// throw would not reach an enclosing `try/catch`.
fn method(name: &str, args: Vec<AbiType>, ret: AbiType, symbol: &str, ts: &str, fp: *const u8) -> Member {
    let mut full = vec![Handle];
    full.extend(args);
    m(name, MemberKind::InstanceMethod, full, ret, symbol, ts, fp)
}

/// Registers the `Socket` class + the `node:dgram` module.
pub fn register(e: &mut Engine) {
    use create as c;
    use emitter as em;
    use lifecycle as lc;
    use mcast as mc;
    use send as sd;
    use tuning as tn;

    let socket = e
        .class("Socket")
        .doc("dgram.Socket — a UDP socket from dgram.createSocket(). An EventEmitter: 'message', 'listening', 'connect', 'close', 'error'.");
    crate::emitter::install(socket, "Socket")
        // ── Lifecycle ──────────────────────────────────────────────────────
        .member(method("bind", vec![], Void, "__RTS_FN_NODE_DGRAM_BIND0", "bind(): void", lc::__RTS_FN_NODE_DGRAM_BIND0 as *const u8))
        .member(method("bind", vec![PolyValue], Void, "__RTS_FN_NODE_DGRAM_BIND1", "bind(port: object): void", lc::__RTS_FN_NODE_DGRAM_BIND1 as *const u8))
        .member(method("bind", vec![PolyValue, PolyValue], Void, "__RTS_FN_NODE_DGRAM_BIND2", "bind(port: object, address: object): void", lc::__RTS_FN_NODE_DGRAM_BIND2 as *const u8))
        .member(method("bind", vec![PolyValue, PolyValue, PolyValue], Void, "__RTS_FN_NODE_DGRAM_BIND3", "bind(port: object, address: object, callback: object): void", lc::__RTS_FN_NODE_DGRAM_BIND3 as *const u8))
        .member(method("close", vec![], Void, "__RTS_FN_NODE_DGRAM_CLOSE0", "close(): void", lc::__RTS_FN_NODE_DGRAM_CLOSE0 as *const u8))
        .member(method("close", vec![PolyValue], Void, "__RTS_FN_NODE_DGRAM_CLOSE1", "close(callback: object): void", lc::__RTS_FN_NODE_DGRAM_CLOSE1 as *const u8))
        .member(method("connect", vec![PolyValue], Void, "__RTS_FN_NODE_DGRAM_CONNECT1", "connect(port: object): void", lc::__RTS_FN_NODE_DGRAM_CONNECT1 as *const u8))
        .member(method("connect", vec![PolyValue, PolyValue], Void, "__RTS_FN_NODE_DGRAM_CONNECT2", "connect(port: object, address: object): void", lc::__RTS_FN_NODE_DGRAM_CONNECT2 as *const u8))
        .member(method("connect", vec![PolyValue, PolyValue, PolyValue], Void, "__RTS_FN_NODE_DGRAM_CONNECT3", "connect(port: object, address: object, callback: object): void", lc::__RTS_FN_NODE_DGRAM_CONNECT3 as *const u8))
        .member(method("disconnect", vec![], Void, "__RTS_FN_NODE_DGRAM_DISCONNECT", "disconnect(): void", lc::__RTS_FN_NODE_DGRAM_DISCONNECT as *const u8))
        // ── Addressing ─────────────────────────────────────────────────────
        .member(method("address", vec![], Handle, "__RTS_FN_NODE_DGRAM_ADDRESS", "address(): object", lc::__RTS_FN_NODE_DGRAM_ADDRESS as *const u8))
        .member(method("remoteAddress", vec![], Handle, "__RTS_FN_NODE_DGRAM_REMOTE_ADDRESS", "remoteAddress(): object", lc::__RTS_FN_NODE_DGRAM_REMOTE_ADDRESS as *const u8))
        // ── Sending ────────────────────────────────────────────────────────
        .member(method("send", vec![PolyValue], Void, "__RTS_FN_NODE_DGRAM_SEND1", "send(msg: object): void", sd::__RTS_FN_NODE_DGRAM_SEND1 as *const u8))
        .member(method("send", vec![PolyValue, PolyValue], Void, "__RTS_FN_NODE_DGRAM_SEND2", "send(msg: object, port: object): void", sd::__RTS_FN_NODE_DGRAM_SEND2 as *const u8))
        .member(method("send", vec![PolyValue, PolyValue, PolyValue], Void, "__RTS_FN_NODE_DGRAM_SEND3", "send(msg: object, port: object, address: object): void", sd::__RTS_FN_NODE_DGRAM_SEND3 as *const u8))
        .member(method("send", vec![PolyValue, PolyValue, PolyValue, PolyValue], Void, "__RTS_FN_NODE_DGRAM_SEND4", "send(msg: object, port: object, address: object, callback: object): void", sd::__RTS_FN_NODE_DGRAM_SEND4 as *const u8))
        .member(method("send", vec![PolyValue, PolyValue, PolyValue, PolyValue, PolyValue], Void, "__RTS_FN_NODE_DGRAM_SEND5", "send(msg: object, offset: object, length: object, port: object, address: object): void", sd::__RTS_FN_NODE_DGRAM_SEND5 as *const u8))
        .member(method("send", vec![PolyValue, PolyValue, PolyValue, PolyValue, PolyValue, PolyValue], Void, "__RTS_FN_NODE_DGRAM_SEND6", "send(msg: object, offset: object, length: object, port: object, address: object, callback: object): void", sd::__RTS_FN_NODE_DGRAM_SEND6 as *const u8))
        // ── Multicast membership ───────────────────────────────────────────
        .member(method("addMembership", vec![StrPtr], Void, "__RTS_FN_NODE_DGRAM_ADD_MEMBERSHIP", "addMembership(multicastAddress: string): void", mc::__RTS_FN_NODE_DGRAM_ADD_MEMBERSHIP as *const u8))
        .member(method("addMembership", vec![StrPtr, StrPtr], Void, "__RTS_FN_NODE_DGRAM_ADD_MEMBERSHIP_IF", "addMembership(multicastAddress: string, multicastInterface: string): void", mc::__RTS_FN_NODE_DGRAM_ADD_MEMBERSHIP_IF as *const u8))
        .member(method("dropMembership", vec![StrPtr], Void, "__RTS_FN_NODE_DGRAM_DROP_MEMBERSHIP", "dropMembership(multicastAddress: string): void", mc::__RTS_FN_NODE_DGRAM_DROP_MEMBERSHIP as *const u8))
        .member(method("dropMembership", vec![StrPtr, StrPtr], Void, "__RTS_FN_NODE_DGRAM_DROP_MEMBERSHIP_IF", "dropMembership(multicastAddress: string, multicastInterface: string): void", mc::__RTS_FN_NODE_DGRAM_DROP_MEMBERSHIP_IF as *const u8))
        .member(method("addSourceSpecificMembership", vec![StrPtr, StrPtr], Void, "__RTS_FN_NODE_DGRAM_ADD_SSM", "addSourceSpecificMembership(sourceAddress: string, groupAddress: string): void", mc::__RTS_FN_NODE_DGRAM_ADD_SSM as *const u8))
        .member(method("addSourceSpecificMembership", vec![StrPtr, StrPtr, StrPtr], Void, "__RTS_FN_NODE_DGRAM_ADD_SSM_IF", "addSourceSpecificMembership(sourceAddress: string, groupAddress: string, multicastInterface: string): void", mc::__RTS_FN_NODE_DGRAM_ADD_SSM_IF as *const u8))
        .member(method("dropSourceSpecificMembership", vec![StrPtr, StrPtr], Void, "__RTS_FN_NODE_DGRAM_DROP_SSM", "dropSourceSpecificMembership(sourceAddress: string, groupAddress: string): void", mc::__RTS_FN_NODE_DGRAM_DROP_SSM as *const u8))
        .member(method("dropSourceSpecificMembership", vec![StrPtr, StrPtr, StrPtr], Void, "__RTS_FN_NODE_DGRAM_DROP_SSM_IF", "dropSourceSpecificMembership(sourceAddress: string, groupAddress: string, multicastInterface: string): void", mc::__RTS_FN_NODE_DGRAM_DROP_SSM_IF as *const u8))
        // ── Tuning ─────────────────────────────────────────────────────────
        .member(method("setTTL", vec![I64], Void, "__RTS_FN_NODE_DGRAM_SET_TTL", "setTTL(ttl: number): void", tn::__RTS_FN_NODE_DGRAM_SET_TTL as *const u8))
        .member(method("setMulticastTTL", vec![I64], Void, "__RTS_FN_NODE_DGRAM_SET_MULTICAST_TTL", "setMulticastTTL(ttl: number): void", tn::__RTS_FN_NODE_DGRAM_SET_MULTICAST_TTL as *const u8))
        .member(method("setMulticastLoopback", vec![Bool], Void, "__RTS_FN_NODE_DGRAM_SET_MULTICAST_LOOPBACK", "setMulticastLoopback(flag: boolean): void", tn::__RTS_FN_NODE_DGRAM_SET_MULTICAST_LOOPBACK as *const u8))
        .member(method("setBroadcast", vec![Bool], Void, "__RTS_FN_NODE_DGRAM_SET_BROADCAST", "setBroadcast(flag: boolean): void", tn::__RTS_FN_NODE_DGRAM_SET_BROADCAST as *const u8))
        .member(method("setMulticastInterface", vec![StrPtr], Void, "__RTS_FN_NODE_DGRAM_SET_MULTICAST_IF", "setMulticastInterface(multicastInterface: string): void", mc::__RTS_FN_NODE_DGRAM_SET_MULTICAST_IF as *const u8))
        .member(method("setSendBufferSize", vec![I64], Void, "__RTS_FN_NODE_DGRAM_SET_SEND_BUFFER_SIZE", "setSendBufferSize(size: number): void", tn::__RTS_FN_NODE_DGRAM_SET_SEND_BUFFER_SIZE as *const u8))
        .member(method("setRecvBufferSize", vec![I64], Void, "__RTS_FN_NODE_DGRAM_SET_RECV_BUFFER_SIZE", "setRecvBufferSize(size: number): void", tn::__RTS_FN_NODE_DGRAM_SET_RECV_BUFFER_SIZE as *const u8))
        .member(method("getSendBufferSize", vec![], I64, "__RTS_FN_NODE_DGRAM_GET_SEND_BUFFER_SIZE", "getSendBufferSize(): number", tn::__RTS_FN_NODE_DGRAM_GET_SEND_BUFFER_SIZE as *const u8))
        .member(method("getRecvBufferSize", vec![], I64, "__RTS_FN_NODE_DGRAM_GET_RECV_BUFFER_SIZE", "getRecvBufferSize(): number", tn::__RTS_FN_NODE_DGRAM_GET_RECV_BUFFER_SIZE as *const u8))
        .member(method("getSendQueueSize", vec![], I64, "__RTS_FN_NODE_DGRAM_GET_SEND_QUEUE_SIZE", "getSendQueueSize(): number", tn::__RTS_FN_NODE_DGRAM_GET_SEND_QUEUE_SIZE as *const u8))
        .member(method("getSendQueueCount", vec![], I64, "__RTS_FN_NODE_DGRAM_GET_SEND_QUEUE_COUNT", "getSendQueueCount(): number", tn::__RTS_FN_NODE_DGRAM_GET_SEND_QUEUE_COUNT as *const u8))
        // ── Reference counting (chainable) ─────────────────────────────────
        .member(method("ref", vec![], Handle, "__RTS_FN_NODE_DGRAM_REF", "ref(): Socket", lc::__RTS_FN_NODE_DGRAM_REF as *const u8))
        .member(method("unref", vec![], Handle, "__RTS_FN_NODE_DGRAM_UNREF", "unref(): Socket", lc::__RTS_FN_NODE_DGRAM_UNREF as *const u8))
        // ── EventEmitter surface (dgram.Socket extends EventEmitter) ───────
        // on/once/off/prepend*/removeAllListeners/listeners/eventNames/max — the
        // crate-shared emitter installs them (its externs take the receiver, so
        // one implementation serves every node class that is an emitter).
        .member(method("emit", vec![StrPtr], Bool, "__RTS_FN_NODE_DGRAM_EMIT0", "emit(event: string): boolean", em::__RTS_FN_NODE_DGRAM_EMIT0 as *const u8))
        .member(method("emit", vec![StrPtr, PolyValue], Bool, "__RTS_FN_NODE_DGRAM_EMIT1", "emit(event: string, a0: object): boolean", em::__RTS_FN_NODE_DGRAM_EMIT1 as *const u8))
        .member(method("emit", vec![StrPtr, PolyValue, PolyValue], Bool, "__RTS_FN_NODE_DGRAM_EMIT2", "emit(event: string, a0: object, a1: object): boolean", em::__RTS_FN_NODE_DGRAM_EMIT2 as *const u8))
        .done();

    e.ns("node:dgram")
        .doc(
            "UDP datagram sockets (node:dgram): createSocket(type | options[, listener]) → a \
             Socket with bind/send/connect/close, multicast membership, TTL/broadcast/buffer \
             tuning and the 'message'/'listening'/'connect'/'close'/'error' events.",
        )
        .member(m("createSocket", MemberKind::Function, vec![PolyValue], Handle, "__RTS_FN_NODE_DGRAM_CREATE_SOCKET", "createSocket(type: object): Socket", c::__RTS_FN_NODE_DGRAM_CREATE_SOCKET as *const u8))
        .member(m("createSocket", MemberKind::Function, vec![PolyValue, PolyValue], Handle, "__RTS_FN_NODE_DGRAM_CREATE_SOCKET_CB", "createSocket(type: object, listener: object): Socket", c::__RTS_FN_NODE_DGRAM_CREATE_SOCKET_CB as *const u8))
        .done();
}
