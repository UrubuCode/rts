//! `node:dns` — name resolution over the OS resolver (`std::net` getaddrinfo).
//! `lookup(hostname, cb)` → `cb(err, address, family)`; `resolve4`/`resolve6`
//! → `cb(err, addresses)`; `getServers()`. The resolution is synchronous and the
//! result is delivered through the codegen callback bridge (the timers/events
//! bridge) — real DNS, no fabricated addresses.
//!
//! The callback must be a function VALUE the bridge can invoke; a NON-capturing
//! named function or arrow works (a capturing closure passed straight to a
//! builtin has the same reification caveat noted for diagnostics_channel).
//!
//! Deferred (need a full DNS-protocol resolver / async event loop): the record
//! families beyond A/AAAA (`resolveMx`/`Txt`/`Srv`/`Ns`/`Cname`/`Soa`/`Ptr`/
//! `Naptr`/`Caa` + `resolveAny`), `reverse`, `getServers`/`setServers`/
//! `setDefaultResultOrder`, `lookupService`, the `dns.promises` API + the
//! `Resolver` class, and the options objects (`{ family, all, hints }`).
//!
//! Layout: `symbols` (extern points), `mod` (registration).

mod symbols;

use rts_engine::AbiType::{self, PolyValue, StrPtr, Void};
use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

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
        ret_class: None,
        pure: false,
        emit: None,
    }
}

/// Registers the `node:dns` surface.
pub fn register(e: &mut Engine) {
    use symbols as s;
    e.ns("node:dns")
        .doc("DNS resolution (node:dns): lookup, resolve4, resolve6.")
        .member(func("lookup", vec![StrPtr, PolyValue], Void, "__RTS_FN_NODE_DNS_LOOKUP", "lookup(hostname: string, callback: object): void", s::__RTS_FN_NODE_DNS_LOOKUP as *const u8))
        .member(func("resolve4", vec![StrPtr, PolyValue], Void, "__RTS_FN_NODE_DNS_RESOLVE4", "resolve4(hostname: string, callback: object): void", s::__RTS_FN_NODE_DNS_RESOLVE4 as *const u8))
        .member(func("resolve6", vec![StrPtr, PolyValue], Void, "__RTS_FN_NODE_DNS_RESOLVE6", "resolve6(hostname: string, callback: object): void", s::__RTS_FN_NODE_DNS_RESOLVE6 as *const u8))
        .done();
}
