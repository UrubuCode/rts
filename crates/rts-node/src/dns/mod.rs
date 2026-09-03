//! `node:dns` (+ `node:dns/promises`) — hostname/address resolution.
//!
//! # Reuse-check
//!
//! `reuse-check` was run before writing this: `rts-cranelift` owns nothing
//! about DNS or sockets, and `rts-core`'s `entry` API (`entry/mod.rs`,
//! `entry/modules.rs`) exposes no name resolution of any kind — the nearest
//! thing to reuse was `entry::settled`, which every promise this module
//! returns goes through, and `entry::call`, which the callback forms use to
//! invoke a user function synchronously (the same primitive `events.rs`'s
//! `emit` calls a listener through). A workspace-wide grep for
//! `hickory`/`trust-dns`/"dns"/"resolver" across every `Cargo.toml` (run
//! again when [`resolve`] was added) found no existing DNS-protocol client
//! anywhere in this repository — `resolve4` is the first one, added rather
//! than assembled from a part that was already here. Nothing else here
//! duplicates state or numbering that another crate already owns.
//!
//! # Two resolution paths, one implemented in full
//!
//! Real Node splits `dns.lookup`/`lookupService` (OS `getaddrinfo`/
//! `getnameinfo`, threadpool-backed) from `dns.resolve*`/`reverse`/
//! `Resolver` (DNS-protocol, speaks to configured servers directly,
//! `hickory-resolver`-backed per `docs/reference/node/crates.md` §4.5).
//! [`lookup`] implements the OS-facility path on `std::net::ToSocketAddrs`.
//! [`resolve`] implements exactly one member of the protocol family —
//! `resolve4` — over a real `hickory-resolver` client; see its own module
//! doc for why only that one member, and for the `localhost` behavior a
//! caller comparing this against `lookup` will notice. The rest of the
//! protocol family (`resolve6`, `resolveMx`, `reverse`, `Resolver`, …) is
//! still refused by name below — this task's subset was `resolve4` alone,
//! and closing one member honestly does not imply the others are trivial
//! extensions of it (each decodes a different `RData` shape).
//!
//! # Synchronous, wearing a callback
//!
//! There is no worker/thread pool here. `lookup` resolves in-line on the
//! calling thread and then calls the callback (or settles the promise)
//! before returning — the same divergence `fs.promises`'s module doc states
//! for file I/O, applied to name resolution: a caller "starting" several
//! lookups "concurrently" does them one at a time, and gets the right answer
//! at the wrong pace. `resolve4` keeps the same contract from the caller's
//! side even though its own client is async underneath — see
//! [`resolve`]'s module doc, "Blocking from the caller's side, a runtime
//! underneath".
//!
//! # Errors are plain objects, not `Error` instances
//!
//! `entry::modules` exposes no way to construct a real `Error` (no
//! `new_error`/`error_class` accessor exists in `rts-core`'s entry API —
//! checked; the reachable class-building primitives are `make_prototype`/
//! `make_instance`, and building an `Error` from those would fabricate a
//! sibling of the one the language already provides for `new Error(...)`,
//! duplicating it rather than reusing it). So a lookup/resolve failure hands
//! back a plain object carrying `code`/`syscall`/`hostname`/`message` —
//! which is what a program checking `err.code === 'ENOTFOUND'` actually
//! reads — and `err instanceof Error` is `false` here where real Node's is
//! `true`. Named rather than silent.
//!
//! # Not implemented, by name
//!
//! - `resolve`/`resolve6`/`resolveAny`/`resolveCaa`/`resolveCname`/
//!   `resolveMx`/`resolveNaptr`/`resolveNs`/`resolvePtr`/`resolveSoa`/
//!   `resolveSrv`/`resolveTlsa`/`resolveTxt`/`reverse`/`dns.Resolver` (and
//!   its `dns.promises` mirror) — `resolve4` (see [`resolve`]) proved the
//!   client works and closed the one member this crate's test corpus
//!   exercises; the rest need their own `RData` decode (`hickory-proto`
//!   names no single method for several of them — see
//!   `docs/reference/node/crates.md` §4.5's own note that
//!   `resolveCname`/`Naptr`/`Ptr`/`Caa`/`Any` have no named client method
//!   and must be decoded off the generic `lookup()`) and are refused by
//!   name rather than half-built.
//! - `lookupService(address, port, callback)` — needs `getnameinfo`-class
//!   reverse lookup (hostname from address) AND a service-name database
//!   (port → name, e.g. `80` → `"http"`); `std` has neither, and hand-rolling
//!   a partial `/etc/services`-style table would be exactly the "plausible
//!   wrong value" the honesty floor refuses (it would also be wrong on
//!   Windows, which has no such file).
//! - The 23 numeric `dns.<CODE>` c-ares error-category constants
//!   (`dns.NODATA`, `dns.FORMERR`, …) — they describe c-ares's own internal
//!   taxonomy and have no canonical value without linking c-ares (or an
//!   equivalent); the `E`-prefixed string codes programs actually branch on
//!   (`ENOTFOUND`, `ETIMEOUT`, `ESERVFAIL`, …) are what `lookup`'s and
//!   `resolve4`'s error objects already carry, so nothing here depends on
//!   the numeric table existing.
//! - `dns.ADDRCONFIG`/`V4MAPPED`/`ALL` ARE implemented as bit flags below,
//!   but they are inert bookkeeping: `lookup`'s options accept an
//!   `options.hints` number and never consult it, because `std::net`'s
//!   resolution has no hints-bitmask control to forward it to. A caller
//!   combining them with `|` gets a number back from that expression and
//!   nothing more.

mod common;
mod lookup;
mod resolve;
mod state;

use rts_core::entry::{self, Context, Provided};

/// The namespace `node:dns` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("lookup", lookup::lookup),
        ("resolve4", resolve::resolve4),
        ("getServers", state::get_servers),
        ("setServers", state::set_servers),
        ("setDefaultResultOrder", state::set_default_result_order),
        ("getDefaultResultOrder", state::get_default_result_order),
        ("setLocalAddress", state::set_local_address),
    ];
    let namespace = entry::make_namespace(context, members);
    let promises = promises_namespace(context);
    entry::put_member(context, namespace, "promises", promises);
    for (name, value) in [("ADDRCONFIG", 4.0), ("V4MAPPED", 8.0), ("ALL", 16.0)] {
        let held = entry::make_number(value);
        entry::put_member(context, namespace, name, held);
        entry::put_member(context, promises, name, held);
    }
    namespace
}

/// `node:dns/promises` (also reachable as `dns.promises`) — the promise
/// mirror of what `namespace` implements. Built separately from `namespace`
/// rather than derived from it, the same way [`super::path`]'s `win32`/
/// `posix` children are each their own function table: `lookup`'s two forms
/// (callback vs. promise) are different enough in shape (a settled
/// `Promise` vs. a synchronous call into a callback) to not share one
/// native body cheaply.
///
/// `resolve4` is NOT a member here: this task's test corpus only imports
/// `resolve4` from `node:dns` (the callback form), and "close a subset in
/// full, not partially" applies to which FORMS of a name are wired, not
/// only to which names are — a `dns.promises.resolve4` that nothing
/// exercises is exactly the unverified surface `reuse-check` exists to keep
/// out.
pub fn promises_namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("lookup", lookup::promise_lookup),
        ("getServers", state::get_servers),
        ("setServers", state::set_servers),
        ("setDefaultResultOrder", state::set_default_result_order),
        ("getDefaultResultOrder", state::get_default_result_order),
        ("setLocalAddress", state::set_local_address),
    ];
    entry::make_namespace(context, members)
}
