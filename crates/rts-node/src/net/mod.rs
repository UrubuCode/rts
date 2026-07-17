//! `node:net` — TCP servers and sockets, plus the socket-independent surface.
//! Real OS sockets, real connections; nothing here is simulated.
//!
//! - `Server`/`Socket` over real TCP + `createServer`/`connect`/
//!   `createConnection` (see [`tcp`]): an accept thread and a read thread queue
//!   plain data, and the event loop's pump builds the JS values on the JS
//!   thread — the model `node:dgram` proved.
//! - `BlockList` (IP allow/deny rule sets, with Node's own cross-family
//!   semantics — see [`blocklist::rules`]) — also what `node:dgram`'s
//!   `receiveBlockList`/`sendBlockList` take.
//! - `SocketAddress`, the immutable value class, and the classifiers
//!   `isIP`/`isIPv4`/`isIPv6`.
//!
//! NOT here: the `stream.Duplex` surface `net.Socket` inherits in Node
//! (`pipe`/`read`/`highWaterMark`/…) — `node:stream` does not exist yet.
//! Everything the spec documents as `Socket`'s OWN surface is implemented.
//! IPC (`path`) and the `fd`/`onread`/`signal`/`lookup` options are REFUSED
//! (`ERR_INVALID_ARG_VALUE`), never silently ignored. Full accounting, including
//! the engine gaps this found: docs/node-implementation/net.md §8.
//!
//! Layout: `ip` (classifiers), `blocklist/` (`mod` = the class, `rules` = the
//! rule engine), `socket_address` (the value class), `tcp/` (Server + Socket),
//! `mod` (shared address helpers + registration).

pub mod blocklist;
mod ip;
mod socket_address;
mod tcp;

use std::net::SocketAddr;

use rts_engine::AbiType::{self, Bool, F64, Handle, I32, PolyValue, StrPtr, Void};
use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// `"IPv4"`/`"IPv6"` — how Node renders a family in an `AddressInfo`.
pub fn family_name(addr: &SocketAddr) -> &'static str {
    if addr.is_ipv6() {
        "IPv6"
    } else {
        "IPv4"
    }
}

/// `{ address, family, port }` — the `AddressInfo` shape `server.address()` and
/// `socket.address()` return.
pub fn address_info(addr: &SocketAddr) -> u64 {
    use rts_engine::heap::shapes::{alloc_shaped_object, string_word};
    alloc_shaped_object(
        &["address", "family", "port"],
        &[
            string_word(addr.ip().to_string().as_bytes()) as i64,
            string_word(family_name(addr).as_bytes()) as i64,
            f64::from(addr.port()).to_bits() as i64,
        ],
    )
}

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

/// An instance member (`this` is the leading `Handle`).
fn method(name: &str, args: Vec<AbiType>, ret: AbiType, symbol: &str, ts: &str, fp: *const u8) -> Member {
    let mut full = vec![Handle];
    full.extend(args);
    m(name, MemberKind::InstanceMethod, full, ret, symbol, ts, fp)
}

/// A read-only property.
fn getter(name: &str, ret: AbiType, symbol: &str, ts: &str, fp: *const u8) -> Member {
    m(name, MemberKind::InstanceGetter, vec![Handle], ret, symbol, ts, fp)
}

/// A writable property (`server.maxConnections = 1`). A getter with no setter is
/// read-only, which is what every other property here wants.
fn setter(name: &str, arg: AbiType, symbol: &str, ts: &str, fp: *const u8) -> Member {
    m(name, MemberKind::InstanceSetter, vec![Handle, arg], Void, symbol, ts, fp)
}

/// A pure classifier (no throw, no side effect).
fn pure_fn(name: &str, args: Vec<AbiType>, ret: AbiType, symbol: &str, ts: &str, fp: *const u8) -> Member {
    let mut member = m(name, MemberKind::Function, args, ret, symbol, ts, fp);
    member.flags = MemberFlags::NONE;
    member.pure = true;
    member
}

/// Registers the `BlockList` + `SocketAddress` classes and the `node:net` module.
pub fn register(e: &mut Engine) {
    use blocklist as bl;
    use socket_address as sa;
    use MemberKind::{Constructor, StaticMethod};

    e.class(bl::CLASS)
        .doc("net.BlockList — rules for blocking IP addresses (node:net).")
        .member(m("new", Constructor, vec![], Handle, "__RTS_FN_NODE_NET_BLOCKLIST_NEW", "new BlockList(): BlockList", bl::__RTS_FN_NODE_NET_BLOCKLIST_NEW as *const u8))
        .member(method("addAddress", vec![PolyValue], Void, "__RTS_FN_NODE_NET_BLOCKLIST_ADD_ADDRESS", "addAddress(address: object): void", bl::__RTS_FN_NODE_NET_BLOCKLIST_ADD_ADDRESS as *const u8))
        .member(method("addAddress", vec![PolyValue, StrPtr], Void, "__RTS_FN_NODE_NET_BLOCKLIST_ADD_ADDRESS_T", "addAddress(address: object, type: string): void", bl::__RTS_FN_NODE_NET_BLOCKLIST_ADD_ADDRESS_T as *const u8))
        .member(method("addRange", vec![PolyValue, PolyValue], Void, "__RTS_FN_NODE_NET_BLOCKLIST_ADD_RANGE", "addRange(start: object, end: object): void", bl::__RTS_FN_NODE_NET_BLOCKLIST_ADD_RANGE as *const u8))
        .member(method("addRange", vec![PolyValue, PolyValue, StrPtr], Void, "__RTS_FN_NODE_NET_BLOCKLIST_ADD_RANGE_T", "addRange(start: object, end: object, type: string): void", bl::__RTS_FN_NODE_NET_BLOCKLIST_ADD_RANGE_T as *const u8))
        .member(method("addSubnet", vec![PolyValue, F64], Void, "__RTS_FN_NODE_NET_BLOCKLIST_ADD_SUBNET", "addSubnet(net: object, prefix: number): void", bl::__RTS_FN_NODE_NET_BLOCKLIST_ADD_SUBNET as *const u8))
        .member(method("addSubnet", vec![PolyValue, F64, StrPtr], Void, "__RTS_FN_NODE_NET_BLOCKLIST_ADD_SUBNET_T", "addSubnet(net: object, prefix: number, type: string): void", bl::__RTS_FN_NODE_NET_BLOCKLIST_ADD_SUBNET_T as *const u8))
        .member(method("check", vec![PolyValue], Bool, "__RTS_FN_NODE_NET_BLOCKLIST_CHECK", "check(address: object): boolean", bl::__RTS_FN_NODE_NET_BLOCKLIST_CHECK as *const u8))
        .member(method("check", vec![PolyValue, StrPtr], Bool, "__RTS_FN_NODE_NET_BLOCKLIST_CHECK_T", "check(address: object, type: string): boolean", bl::__RTS_FN_NODE_NET_BLOCKLIST_CHECK_T as *const u8))
        .member(method("toJSON", vec![], Handle, "__RTS_FN_NODE_NET_BLOCKLIST_RULES", "toJSON(): string[]", bl::__RTS_FN_NODE_NET_BLOCKLIST_RULES as *const u8))
        .member(method("fromJSON", vec![PolyValue], Void, "__RTS_FN_NODE_NET_BLOCKLIST_FROM_JSON", "fromJSON(value: object): void", bl::__RTS_FN_NODE_NET_BLOCKLIST_FROM_JSON as *const u8))
        .member(getter("rules", Handle, "__RTS_FN_NODE_NET_BLOCKLIST_RULES", "rules: string[]", bl::__RTS_FN_NODE_NET_BLOCKLIST_RULES as *const u8))
        .member(m("isBlockList", StaticMethod, vec![PolyValue], Bool, "__RTS_FN_NODE_NET_BLOCKLIST_IS_BLOCK_LIST", "isBlockList(value: object): boolean", bl::__RTS_FN_NODE_NET_BLOCKLIST_IS_BLOCK_LIST as *const u8))
        .done();

    e.class(sa::CLASS)
        .doc("net.SocketAddress — an immutable { address, family, port, flowlabel } (node:net).")
        .member(m("new", Constructor, vec![], Handle, "__RTS_FN_NODE_NET_SOCKET_ADDRESS_NEW", "new SocketAddress(): SocketAddress", sa::__RTS_FN_NODE_NET_SOCKET_ADDRESS_NEW as *const u8))
        .member(m("new", Constructor, vec![Handle], Handle, "__RTS_FN_NODE_NET_SOCKET_ADDRESS_NEW_OPTS", "new SocketAddress(options: object): SocketAddress", sa::__RTS_FN_NODE_NET_SOCKET_ADDRESS_NEW_OPTS as *const u8))
        .member(m("parse", StaticMethod, vec![StrPtr], Handle, "__RTS_FN_NODE_NET_SOCKET_ADDRESS_PARSE", "parse(input: string): SocketAddress", sa::__RTS_FN_NODE_NET_SOCKET_ADDRESS_PARSE as *const u8))
        .member(getter("address", Handle, "__RTS_FN_NODE_NET_SOCKET_ADDRESS_ADDRESS", "address: string", sa::__RTS_FN_NODE_NET_SOCKET_ADDRESS_ADDRESS as *const u8))
        .member(getter("family", Handle, "__RTS_FN_NODE_NET_SOCKET_ADDRESS_FAMILY", "family: string", sa::__RTS_FN_NODE_NET_SOCKET_ADDRESS_FAMILY as *const u8))
        .member(getter("port", F64, "__RTS_FN_NODE_NET_SOCKET_ADDRESS_PORT", "port: number", sa::__RTS_FN_NODE_NET_SOCKET_ADDRESS_PORT as *const u8))
        .member(getter("flowlabel", F64, "__RTS_FN_NODE_NET_SOCKET_ADDRESS_FLOWLABEL", "flowlabel: number", sa::__RTS_FN_NODE_NET_SOCKET_ADDRESS_FLOWLABEL as *const u8))
        .done();

    register_server(e);
    register_socket(e);

    use tcp::opts as cfg;
    use tcp::server as sv;
    use tcp::socket as sk;
    e.ns("node:net")
        .doc(
            "Networking (node:net): createServer/connect/createConnection + the Server and Socket \
             classes over real TCP, the IP classifiers isIP/isIPv4/isIPv6, the BlockList rule-set \
             class, and the immutable SocketAddress value class.",
        )
        .member(pure_fn("isIP", vec![StrPtr], I32, "__RTS_FN_NODE_NET_IS_IP", "isIP(input: string): number", ip::__RTS_FN_NODE_NET_IS_IP as *const u8))
        .member(pure_fn("isIPv4", vec![StrPtr], Bool, "__RTS_FN_NODE_NET_IS_IPV4", "isIPv4(input: string): boolean", ip::__RTS_FN_NODE_NET_IS_IPV4 as *const u8))
        .member(pure_fn("isIPv6", vec![StrPtr], Bool, "__RTS_FN_NODE_NET_IS_IPV6", "isIPv6(input: string): boolean", ip::__RTS_FN_NODE_NET_IS_IPV6 as *const u8))
        // createServer([options][, connectionListener]) — overloaded BY VALUE.
        .member(m("createServer", MemberKind::Function, vec![], Handle, "__RTS_FN_NODE_NET_CREATE_SERVER", "createServer(): Server", sv::__RTS_FN_NODE_NET_CREATE_SERVER as *const u8))
        .member(m("createServer", MemberKind::Function, vec![PolyValue], Handle, "__RTS_FN_NODE_NET_CREATE_SERVER1", "createServer(options: object): Server", sv::__RTS_FN_NODE_NET_CREATE_SERVER1 as *const u8))
        .member(m("createServer", MemberKind::Function, vec![PolyValue, PolyValue], Handle, "__RTS_FN_NODE_NET_CREATE_SERVER2", "createServer(options: object, connectionListener: object): Server", sv::__RTS_FN_NODE_NET_CREATE_SERVER2 as *const u8))
        // connect / createConnection — pure aliases of one another in Node.
        .member(m("createConnection", MemberKind::Function, vec![PolyValue], Handle, "__RTS_FN_NODE_NET_CONNECT1", "createConnection(port: object): Socket", __RTS_FN_NODE_NET_CONNECT1 as *const u8))
        .member(m("createConnection", MemberKind::Function, vec![PolyValue, PolyValue], Handle, "__RTS_FN_NODE_NET_CONNECT2", "createConnection(port: object, host: object): Socket", __RTS_FN_NODE_NET_CONNECT2 as *const u8))
        .member(m("createConnection", MemberKind::Function, vec![PolyValue, PolyValue, PolyValue], Handle, "__RTS_FN_NODE_NET_CONNECT3", "createConnection(port: object, host: object, connectListener: object): Socket", __RTS_FN_NODE_NET_CONNECT3 as *const u8))
        .member(m("connect", MemberKind::Function, vec![PolyValue], Handle, "__RTS_FN_NODE_NET_CONNECT1", "connect(port: object): Socket", __RTS_FN_NODE_NET_CONNECT1 as *const u8))
        .member(m("connect", MemberKind::Function, vec![PolyValue, PolyValue], Handle, "__RTS_FN_NODE_NET_CONNECT2", "connect(port: object, host: object): Socket", __RTS_FN_NODE_NET_CONNECT2 as *const u8))
        .member(m("connect", MemberKind::Function, vec![PolyValue, PolyValue, PolyValue], Handle, "__RTS_FN_NODE_NET_CONNECT3", "connect(port: object, host: object, connectListener: object): Socket", __RTS_FN_NODE_NET_CONNECT3 as *const u8))
        // Process-wide autoSelectFamily config (Node keeps these global).
        .member(pure_fn("getDefaultAutoSelectFamily", vec![], Bool, "__RTS_FN_NODE_NET_GET_ASF", "getDefaultAutoSelectFamily(): boolean", __RTS_FN_NODE_NET_GET_ASF as *const u8))
        .member(m("setDefaultAutoSelectFamily", MemberKind::Function, vec![Bool], Void, "__RTS_FN_NODE_NET_SET_ASF", "setDefaultAutoSelectFamily(value: boolean): void", __RTS_FN_NODE_NET_SET_ASF as *const u8))
        .member(pure_fn("getDefaultAutoSelectFamilyAttemptTimeout", vec![], F64, "__RTS_FN_NODE_NET_GET_ASF_TIMEOUT", "getDefaultAutoSelectFamilyAttemptTimeout(): number", __RTS_FN_NODE_NET_GET_ASF_TIMEOUT as *const u8))
        .member(m("setDefaultAutoSelectFamilyAttemptTimeout", MemberKind::Function, vec![F64], Void, "__RTS_FN_NODE_NET_SET_ASF_TIMEOUT", "setDefaultAutoSelectFamilyAttemptTimeout(value: number): void", __RTS_FN_NODE_NET_SET_ASF_TIMEOUT as *const u8))
        .done();
    let _ = (cfg::auto_select_family, sk::__RTS_FN_NODE_NET_SOCKET_NEW);
}

/// `net.connect(...)`/`net.createConnection(...)` — build a `Socket` and start
/// connecting; the same overloads `socket.connect` takes.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_CONNECT1(a0: u64) -> u64 {
    let s = tcp::socket::__RTS_FN_NODE_NET_SOCKET_NEW();
    tcp::socket::__RTS_FN_NODE_NET_SOCKET_CONNECT1(s, a0);
    s
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_CONNECT2(a0: u64, a1: u64) -> u64 {
    let s = tcp::socket::__RTS_FN_NODE_NET_SOCKET_NEW();
    tcp::socket::__RTS_FN_NODE_NET_SOCKET_CONNECT2(s, a0, a1);
    s
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_CONNECT3(a0: u64, a1: u64, a2: u64) -> u64 {
    let s = tcp::socket::__RTS_FN_NODE_NET_SOCKET_NEW();
    tcp::socket::__RTS_FN_NODE_NET_SOCKET_CONNECT3(s, a0, a1, a2);
    s
}

/// `net.getDefaultAutoSelectFamily()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_GET_ASF() -> i64 {
    i64::from(tcp::opts::auto_select_family())
}

/// `net.setDefaultAutoSelectFamily(value)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SET_ASF(v: i64) {
    tcp::opts::set_auto_select_family(v != 0);
}

/// `net.getDefaultAutoSelectFamilyAttemptTimeout()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_GET_ASF_TIMEOUT() -> f64 {
    tcp::opts::auto_select_family_timeout() as f64
}

/// `net.setDefaultAutoSelectFamilyAttemptTimeout(value)` — values < 10 clamp up.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SET_ASF_TIMEOUT(ms: f64) {
    tcp::opts::set_auto_select_family_timeout(ms.max(0.0) as i64);
}

/// The `Server` class.
fn register_server(e: &mut Engine) {
    use tcp::server as sv;
    let class = e
        .class(sv::CLASS)
        .doc("net.Server — a TCP server from net.createServer(). An EventEmitter: 'connection', 'listening', 'close', 'error', 'drop'.");
    crate::emitter::install(class, sv::CLASS)
        .member(method("listen", vec![], Handle, "__RTS_FN_NODE_NET_SERVER_LISTEN0", "listen(): Server", sv::__RTS_FN_NODE_NET_SERVER_LISTEN0 as *const u8))
        .member(method("listen", vec![PolyValue], Handle, "__RTS_FN_NODE_NET_SERVER_LISTEN1", "listen(port: object): Server", sv::__RTS_FN_NODE_NET_SERVER_LISTEN1 as *const u8))
        .member(method("listen", vec![PolyValue, PolyValue], Handle, "__RTS_FN_NODE_NET_SERVER_LISTEN2", "listen(port: object, host: object): Server", sv::__RTS_FN_NODE_NET_SERVER_LISTEN2 as *const u8))
        .member(method("listen", vec![PolyValue, PolyValue, PolyValue], Handle, "__RTS_FN_NODE_NET_SERVER_LISTEN3", "listen(port: object, host: object, backlog: object): Server", sv::__RTS_FN_NODE_NET_SERVER_LISTEN3 as *const u8))
        .member(method("listen", vec![PolyValue, PolyValue, PolyValue, PolyValue], Handle, "__RTS_FN_NODE_NET_SERVER_LISTEN4", "listen(port: object, host: object, backlog: object, callback: object): Server", sv::__RTS_FN_NODE_NET_SERVER_LISTEN4 as *const u8))
        .member(method("close", vec![], Handle, "__RTS_FN_NODE_NET_SERVER_CLOSE0", "close(): Server", sv::__RTS_FN_NODE_NET_SERVER_CLOSE0 as *const u8))
        .member(method("close", vec![PolyValue], Handle, "__RTS_FN_NODE_NET_SERVER_CLOSE1", "close(callback: object): Server", sv::__RTS_FN_NODE_NET_SERVER_CLOSE1 as *const u8))
        .member(method("address", vec![], Handle, "__RTS_FN_NODE_NET_SERVER_ADDRESS", "address(): object", sv::__RTS_FN_NODE_NET_SERVER_ADDRESS as *const u8))
        .member(method("getConnections", vec![PolyValue], Handle, "__RTS_FN_NODE_NET_SERVER_GET_CONNECTIONS", "getConnections(callback: object): Server", sv::__RTS_FN_NODE_NET_SERVER_GET_CONNECTIONS as *const u8))
        .member(method("ref", vec![], Handle, "__RTS_FN_NODE_NET_SERVER_REF", "ref(): Server", sv::__RTS_FN_NODE_NET_SERVER_REF as *const u8))
        .member(method("unref", vec![], Handle, "__RTS_FN_NODE_NET_SERVER_UNREF", "unref(): Server", sv::__RTS_FN_NODE_NET_SERVER_UNREF as *const u8))
        .member(getter("listening", Bool, "__RTS_FN_NODE_NET_SERVER_LISTENING", "listening: boolean", sv::__RTS_FN_NODE_NET_SERVER_LISTENING as *const u8))
        .member(method("emit", vec![StrPtr], Bool, "__RTS_FN_NODE_NET_SERVER_EMIT0", "emit(event: string): boolean", sv::__RTS_FN_NODE_NET_SERVER_EMIT0 as *const u8))
        .member(method("emit", vec![StrPtr, PolyValue], Bool, "__RTS_FN_NODE_NET_SERVER_EMIT1", "emit(event: string, a0: object): boolean", sv::__RTS_FN_NODE_NET_SERVER_EMIT1 as *const u8))
        .done();
}

/// The `Socket` class.
fn register_socket(e: &mut Engine) {
    use tcp::props as pr;
    use tcp::socket as sk;
    let class = e
        .class(sk::CLASS)
        .doc("net.Socket — a TCP connection. An EventEmitter: 'connect', 'ready', 'data', 'end', 'close', 'error', 'timeout', 'drain', 'lookup'.");
    crate::emitter::install(class, sk::CLASS)
        .member(m("new", MemberKind::Constructor, vec![], Handle, "__RTS_FN_NODE_NET_SOCKET_NEW", "new Socket(): Socket", sk::__RTS_FN_NODE_NET_SOCKET_NEW as *const u8))
        .member(m("new", MemberKind::Constructor, vec![Handle], Handle, "__RTS_FN_NODE_NET_SOCKET_NEW_OPTS", "new Socket(options: object): Socket", sk::__RTS_FN_NODE_NET_SOCKET_NEW_OPTS as *const u8))
        .member(method("connect", vec![PolyValue], Handle, "__RTS_FN_NODE_NET_SOCKET_CONNECT1", "connect(port: object): Socket", sk::__RTS_FN_NODE_NET_SOCKET_CONNECT1 as *const u8))
        .member(method("connect", vec![PolyValue, PolyValue], Handle, "__RTS_FN_NODE_NET_SOCKET_CONNECT2", "connect(port: object, host: object): Socket", sk::__RTS_FN_NODE_NET_SOCKET_CONNECT2 as *const u8))
        .member(method("connect", vec![PolyValue, PolyValue, PolyValue], Handle, "__RTS_FN_NODE_NET_SOCKET_CONNECT3", "connect(port: object, host: object, connectListener: object): Socket", sk::__RTS_FN_NODE_NET_SOCKET_CONNECT3 as *const u8))
        .member(method("write", vec![PolyValue], Bool, "__RTS_FN_NODE_NET_SOCKET_WRITE1", "write(data: object): boolean", sk::__RTS_FN_NODE_NET_SOCKET_WRITE1 as *const u8))
        .member(method("write", vec![PolyValue, PolyValue], Bool, "__RTS_FN_NODE_NET_SOCKET_WRITE2", "write(data: object, encoding: object): boolean", sk::__RTS_FN_NODE_NET_SOCKET_WRITE2 as *const u8))
        .member(method("write", vec![PolyValue, PolyValue, PolyValue], Bool, "__RTS_FN_NODE_NET_SOCKET_WRITE3", "write(data: object, encoding: object, callback: object): boolean", sk::__RTS_FN_NODE_NET_SOCKET_WRITE3 as *const u8))
        .member(method("end", vec![], Handle, "__RTS_FN_NODE_NET_SOCKET_END0", "end(): Socket", sk::__RTS_FN_NODE_NET_SOCKET_END0 as *const u8))
        .member(method("end", vec![PolyValue], Handle, "__RTS_FN_NODE_NET_SOCKET_END1", "end(data: object): Socket", sk::__RTS_FN_NODE_NET_SOCKET_END1 as *const u8))
        .member(method("end", vec![PolyValue, PolyValue], Handle, "__RTS_FN_NODE_NET_SOCKET_END2", "end(data: object, encoding: object): Socket", sk::__RTS_FN_NODE_NET_SOCKET_END2 as *const u8))
        .member(method("end", vec![PolyValue, PolyValue, PolyValue], Handle, "__RTS_FN_NODE_NET_SOCKET_END3", "end(data: object, encoding: object, callback: object): Socket", sk::__RTS_FN_NODE_NET_SOCKET_END3 as *const u8))
        .member(method("destroy", vec![], Handle, "__RTS_FN_NODE_NET_SOCKET_DESTROY0", "destroy(): Socket", sk::__RTS_FN_NODE_NET_SOCKET_DESTROY0 as *const u8))
        .member(method("destroy", vec![PolyValue], Handle, "__RTS_FN_NODE_NET_SOCKET_DESTROY1", "destroy(error: object): Socket", sk::__RTS_FN_NODE_NET_SOCKET_DESTROY1 as *const u8))
        .member(method("destroySoon", vec![], Void, "__RTS_FN_NODE_NET_SOCKET_DESTROY_SOON", "destroySoon(): void", sk::__RTS_FN_NODE_NET_SOCKET_DESTROY_SOON as *const u8))
        .member(method("resetAndDestroy", vec![], Handle, "__RTS_FN_NODE_NET_SOCKET_RESET_AND_DESTROY", "resetAndDestroy(): Socket", sk::__RTS_FN_NODE_NET_SOCKET_RESET_AND_DESTROY as *const u8))
        .member(method("address", vec![], Handle, "__RTS_FN_NODE_NET_SOCKET_ADDRESS", "address(): object", pr::__RTS_FN_NODE_NET_SOCKET_ADDRESS as *const u8))
        .member(method("pause", vec![], Handle, "__RTS_FN_NODE_NET_SOCKET_PAUSE", "pause(): Socket", pr::__RTS_FN_NODE_NET_SOCKET_PAUSE as *const u8))
        .member(method("resume", vec![], Handle, "__RTS_FN_NODE_NET_SOCKET_RESUME", "resume(): Socket", pr::__RTS_FN_NODE_NET_SOCKET_RESUME as *const u8))
        .member(method("ref", vec![], Handle, "__RTS_FN_NODE_NET_SOCKET_REF", "ref(): Socket", pr::__RTS_FN_NODE_NET_SOCKET_REF as *const u8))
        .member(method("unref", vec![], Handle, "__RTS_FN_NODE_NET_SOCKET_UNREF", "unref(): Socket", pr::__RTS_FN_NODE_NET_SOCKET_UNREF as *const u8))
        .member(method("setEncoding", vec![StrPtr], Handle, "__RTS_FN_NODE_NET_SOCKET_SET_ENCODING", "setEncoding(encoding: string): Socket", pr::__RTS_FN_NODE_NET_SOCKET_SET_ENCODING as *const u8))
        .member(method("setNoDelay", vec![], Handle, "__RTS_FN_NODE_NET_SOCKET_SET_NO_DELAY_D", "setNoDelay(): Socket", __RTS_FN_NODE_NET_SOCKET_SET_NO_DELAY_D as *const u8))
        .member(method("setNoDelay", vec![Bool], Handle, "__RTS_FN_NODE_NET_SOCKET_SET_NO_DELAY", "setNoDelay(noDelay: boolean): Socket", pr::__RTS_FN_NODE_NET_SOCKET_SET_NO_DELAY as *const u8))
        .member(method("setKeepAlive", vec![], Handle, "__RTS_FN_NODE_NET_SOCKET_SET_KEEP_ALIVE_D", "setKeepAlive(): Socket", __RTS_FN_NODE_NET_SOCKET_SET_KEEP_ALIVE_D as *const u8))
        .member(method("setKeepAlive", vec![Bool], Handle, "__RTS_FN_NODE_NET_SOCKET_SET_KEEP_ALIVE1", "setKeepAlive(enable: boolean): Socket", __RTS_FN_NODE_NET_SOCKET_SET_KEEP_ALIVE1 as *const u8))
        .member(method("setKeepAlive", vec![Bool, F64], Handle, "__RTS_FN_NODE_NET_SOCKET_SET_KEEP_ALIVE", "setKeepAlive(enable: boolean, initialDelay: number): Socket", pr::__RTS_FN_NODE_NET_SOCKET_SET_KEEP_ALIVE as *const u8))
        .member(method("setTimeout", vec![F64], Handle, "__RTS_FN_NODE_NET_SOCKET_SET_TIMEOUT", "setTimeout(timeout: number): Socket", pr::__RTS_FN_NODE_NET_SOCKET_SET_TIMEOUT as *const u8))
        .member(method("setTimeout", vec![F64, PolyValue], Handle, "__RTS_FN_NODE_NET_SOCKET_SET_TIMEOUT_CB", "setTimeout(timeout: number, callback: object): Socket", __RTS_FN_NODE_NET_SOCKET_SET_TIMEOUT_CB as *const u8))
        .member(method("getTypeOfService", vec![], F64, "__RTS_FN_NODE_NET_SOCKET_GET_TOS", "getTypeOfService(): number", pr::__RTS_FN_NODE_NET_SOCKET_GET_TOS as *const u8))
        .member(method("setTypeOfService", vec![F64], Handle, "__RTS_FN_NODE_NET_SOCKET_SET_TOS", "setTypeOfService(tos: number): Socket", pr::__RTS_FN_NODE_NET_SOCKET_SET_TOS as *const u8))
        .member(method("emit", vec![StrPtr], Bool, "__RTS_FN_NODE_NET_SOCKET_EMIT0", "emit(event: string): boolean", sk::__RTS_FN_NODE_NET_SOCKET_EMIT0 as *const u8))
        .member(method("emit", vec![StrPtr, PolyValue], Bool, "__RTS_FN_NODE_NET_SOCKET_EMIT1", "emit(event: string, a0: object): boolean", sk::__RTS_FN_NODE_NET_SOCKET_EMIT1 as *const u8))
        // ── Properties ─────────────────────────────────────────────────────
        .member(getter("remoteAddress", Handle, "__RTS_FN_NODE_NET_SOCKET_REMOTE_ADDRESS", "remoteAddress: string", pr::__RTS_FN_NODE_NET_SOCKET_REMOTE_ADDRESS as *const u8))
        .member(getter("remoteFamily", Handle, "__RTS_FN_NODE_NET_SOCKET_REMOTE_FAMILY", "remoteFamily: string", pr::__RTS_FN_NODE_NET_SOCKET_REMOTE_FAMILY as *const u8))
        .member(getter("remotePort", F64, "__RTS_FN_NODE_NET_SOCKET_REMOTE_PORT", "remotePort: number", pr::__RTS_FN_NODE_NET_SOCKET_REMOTE_PORT as *const u8))
        .member(getter("localAddress", Handle, "__RTS_FN_NODE_NET_SOCKET_LOCAL_ADDRESS", "localAddress: string", pr::__RTS_FN_NODE_NET_SOCKET_LOCAL_ADDRESS as *const u8))
        .member(getter("localFamily", Handle, "__RTS_FN_NODE_NET_SOCKET_LOCAL_FAMILY", "localFamily: string", pr::__RTS_FN_NODE_NET_SOCKET_LOCAL_FAMILY as *const u8))
        .member(getter("localPort", F64, "__RTS_FN_NODE_NET_SOCKET_LOCAL_PORT", "localPort: number", pr::__RTS_FN_NODE_NET_SOCKET_LOCAL_PORT as *const u8))
        .member(getter("bytesRead", F64, "__RTS_FN_NODE_NET_SOCKET_BYTES_READ", "bytesRead: number", pr::__RTS_FN_NODE_NET_SOCKET_BYTES_READ as *const u8))
        .member(getter("bytesWritten", F64, "__RTS_FN_NODE_NET_SOCKET_BYTES_WRITTEN", "bytesWritten: number", pr::__RTS_FN_NODE_NET_SOCKET_BYTES_WRITTEN as *const u8))
        .member(getter("bufferSize", F64, "__RTS_FN_NODE_NET_SOCKET_BUFFER_SIZE", "bufferSize: number", pr::__RTS_FN_NODE_NET_SOCKET_BUFFER_SIZE as *const u8))
        .member(getter("connecting", Bool, "__RTS_FN_NODE_NET_SOCKET_CONNECTING", "connecting: boolean", pr::__RTS_FN_NODE_NET_SOCKET_CONNECTING as *const u8))
        .member(getter("pending", Bool, "__RTS_FN_NODE_NET_SOCKET_PENDING", "pending: boolean", pr::__RTS_FN_NODE_NET_SOCKET_PENDING as *const u8))
        .member(getter("destroyed", Bool, "__RTS_FN_NODE_NET_SOCKET_DESTROYED", "destroyed: boolean", pr::__RTS_FN_NODE_NET_SOCKET_DESTROYED as *const u8))
        .member(getter("readyState", Handle, "__RTS_FN_NODE_NET_SOCKET_READY_STATE", "readyState: string", pr::__RTS_FN_NODE_NET_SOCKET_READY_STATE as *const u8))
        .member(getter("timeout", F64, "__RTS_FN_NODE_NET_SOCKET_GET_TIMEOUT", "timeout: number", pr::__RTS_FN_NODE_NET_SOCKET_GET_TIMEOUT as *const u8))
        .member(getter("autoSelectFamilyAttemptedAddresses", Handle, "__RTS_FN_NODE_NET_SOCKET_ATTEMPTED", "autoSelectFamilyAttemptedAddresses: string[]", pr::__RTS_FN_NODE_NET_SOCKET_ATTEMPTED as *const u8))
        .done();
}

/// `socket.setNoDelay()` — Node's default arg is `true`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_SET_NO_DELAY_D(this: u64) -> u64 {
    tcp::props::__RTS_FN_NODE_NET_SOCKET_SET_NO_DELAY(this, 1)
}

/// `socket.setKeepAlive()` — defaults `enable = false`, `initialDelay = 0`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_SET_KEEP_ALIVE_D(this: u64) -> u64 {
    tcp::props::__RTS_FN_NODE_NET_SOCKET_SET_KEEP_ALIVE(this, 0, 0.0)
}

/// `socket.setKeepAlive(enable)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_SET_KEEP_ALIVE1(this: u64, enable: i64) -> u64 {
    tcp::props::__RTS_FN_NODE_NET_SOCKET_SET_KEEP_ALIVE(this, enable, 0.0)
}

/// `socket.setTimeout(timeout, callback)` — the callback is a one-time
/// `'timeout'` listener.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_SET_TIMEOUT_CB(this: u64, ms: f64, cb: u64) -> u64 {
    if let crate::values::Val::Func(f) = crate::values::val(cb) {
        crate::emitter::add(this, "timeout", f, true, false);
    }
    tcp::props::__RTS_FN_NODE_NET_SOCKET_SET_TIMEOUT(this, ms)
}
