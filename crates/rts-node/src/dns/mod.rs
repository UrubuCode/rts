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
//! Layout: `symbols` (`#[rtse::function]` entry points), `mod` (registration).

mod symbols;

use rts_engine::Engine;

/// Registers the `node:dns` surface.
pub fn register(e: &mut Engine) {
    use symbols as s;
    e.ns("node:dns")
        .doc("DNS resolution (node:dns): lookup, resolve4, resolve6.")
        .member(s::lookup_entry())
        .member(s::resolve4_entry())
        .member(s::resolve6_entry())
        .done();
}
