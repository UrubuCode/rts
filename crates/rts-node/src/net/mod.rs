//! `node:net` — the socket-independent surface, implemented for real:
//! the IP-string classifiers `isIP`/`isIPv4`/`isIPv6`, the **`BlockList`** class
//! (IP allow/deny rule sets, matching Node's own cross-family semantics — see
//! [`blocklist::rules`]), and the immutable **`SocketAddress`** value class.
//!
//! `BlockList` is not decorative: it is what `node:dgram`'s `receiveBlockList`/
//! `sendBlockList` options take, and what `node:net`'s own `Server`/`Socket`
//! will take once they land.
//!
//! Deferred (need the event-loop-driven stream machinery, not this module's
//! shape): the `Socket` and `Server` classes (`createServer`/`connect`/
//! `createConnection` and their `'data'`/`'connection'` events — `net.Socket` is
//! a `stream.Duplex` in Node and `node:stream` does not exist yet), and the
//! `autoSelectFamily` config pair that only means something to `Socket`.
//! Documented in docs/node-implementation/net.md §8.
//!
//! Layout: `ip` (classifiers), `blocklist/` (`mod` = the class, `rules` = the
//! rule engine), `socket_address` (the value class), `mod` (registration).

pub mod blocklist;
mod ip;
mod socket_address;

use rts_engine::AbiType::{self, Bool, F64, Handle, I32, PolyValue, StrPtr, Void};
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

    e.ns("node:net")
        .doc(
            "Network primitives (node:net): the IP classifiers isIP/isIPv4/isIPv6, the BlockList \
             rule-set class, and the immutable SocketAddress value class.",
        )
        .member(pure_fn("isIP", vec![StrPtr], I32, "__RTS_FN_NODE_NET_IS_IP", "isIP(input: string): number", ip::__RTS_FN_NODE_NET_IS_IP as *const u8))
        .member(pure_fn("isIPv4", vec![StrPtr], Bool, "__RTS_FN_NODE_NET_IS_IPV4", "isIPv4(input: string): boolean", ip::__RTS_FN_NODE_NET_IS_IPV4 as *const u8))
        .member(pure_fn("isIPv6", vec![StrPtr], Bool, "__RTS_FN_NODE_NET_IS_IPV6", "isIPv6(input: string): boolean", ip::__RTS_FN_NODE_NET_IS_IPV6 as *const u8))
        .done();
}
