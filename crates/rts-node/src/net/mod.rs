//! `node:net` — the socket-independent surface: the IP-address validators
//! `isIP(input)` (0 / 4 / 6), `isIPv4(input)`, `isIPv6(input)`, backed by the
//! real `std::net` address parsers. No fabrication.
//!
//! Deferred (need the event-loop / socket / stream subsystems): the `Socket`
//! and `Server` classes (`createServer`/`connect`/`createConnection` + their
//! `'data'`/`'connection'`/`'close'` events and readable/writable streams), the
//! `BlockList` class, `getDefaultAutoSelectFamily*` — the whole networked TCP/IPC
//! machinery.
//!
//! Layout: `mod` (validators + registration).

use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

use rts_engine::AbiType::{self, Bool, I32, StrPtr};
use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

fn read(ptr: *const u8, len: i64) -> String {
    unsafe { rts_engine::abi::str_abi::from_abi(ptr, len) }.unwrap_or("").to_string()
}

fn is_v4(s: &str) -> bool {
    Ipv4Addr::from_str(s).is_ok()
}

fn is_v6(s: &str) -> bool {
    Ipv6Addr::from_str(s).is_ok()
}

/// `net.isIP(input)` → 4, 6, or 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_IS_IP(p: *const u8, l: i64) -> i32 {
    let s = read(p, l);
    if is_v4(&s) {
        4
    } else if is_v6(&s) {
        6
    } else {
        0
    }
}

/// `net.isIPv4(input)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_IS_IPV4(p: *const u8, l: i64) -> i64 {
    is_v4(&read(p, l)) as i64
}

/// `net.isIPv6(input)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_IS_IPV6(p: *const u8, l: i64) -> i64 {
    is_v6(&read(p, l)) as i64
}

fn func(name: &str, args: Vec<AbiType>, ret: AbiType, symbol: &str, ts: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig: Sig::new(args, ret),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        pure: true,
        intrinsic: None,
    }
}

/// Registers the `node:net` surface.
pub fn register(e: &mut Engine) {
    e.ns("node:net")
        .doc("Network address validation (node:net): isIP, isIPv4, isIPv6.")
        .member(func("isIP", vec![StrPtr], I32, "__RTS_FN_NODE_NET_IS_IP", "isIP(input: string): number", __RTS_FN_NODE_NET_IS_IP as *const u8))
        .member(func("isIPv4", vec![StrPtr], Bool, "__RTS_FN_NODE_NET_IS_IPV4", "isIPv4(input: string): boolean", __RTS_FN_NODE_NET_IS_IPV4 as *const u8))
        .member(func("isIPv6", vec![StrPtr], Bool, "__RTS_FN_NODE_NET_IS_IPV6", "isIPv6(input: string): boolean", __RTS_FN_NODE_NET_IS_IPV6 as *const u8))
        .done();
}
