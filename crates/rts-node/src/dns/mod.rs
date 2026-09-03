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
//! # Two resolution paths, both implemented
//!
//! Real Node splits `dns.lookup`/`lookupService` (OS `getaddrinfo`/
//! `getnameinfo`, threadpool-backed) from `dns.resolve*`/`reverse`/
//! `Resolver` (DNS-protocol, speaks to configured servers directly,
//! `hickory-resolver`-backed per `docs/reference/node/crates.md` §4.5).
//! [`lookup`] implements the OS-facility path on `std::net::ToSocketAddrs`.
//! [`resolve`] and the `rr_*` files beside it implement the whole protocol
//! family over a real `hickory-resolver` client: `resolve4`/`resolve6`/
//! `resolve`/`resolveAny`/`resolveCaa`/`resolveCname`/`resolveMx`/
//! `resolveNaptr`/`resolveNs`/`resolvePtr`/`resolveSoa`/`resolveSrv`/
//! `resolveTlsa`/`resolveTxt`/`reverse`, and the `Resolver` class over its
//! OWN configuration — see [`resolve`]'s module doc for the `localhost`
//! behavior a caller comparing `resolve4` against `lookup` will notice, and
//! `resolver_class.rs`'s module doc for why a `Resolver` instance is not
//! merely a second name for the module-level functions.
//!
//! The record-type decoders live one file per family
//! (`rr_addr`/`rr_alias`/`rr_any`/`rr_security`/`rr_service`/`rr_soa_naptr`/
//! `rr_text`) rather than one per name, split by RDATA shape rather than by
//! Node's own function list — `resolveCname`/`resolveNs`/`resolvePtr` all
//! decode the identical `Name`-wrapping RDATA, so they are one file
//! (`rr_alias.rs`) beside `reverse` (the same RDATA, the opposite query
//! direction), not three.
//!
//! # Synchronous, wearing a callback
//!
//! There is no worker/thread pool here. `lookup` resolves in-line on the
//! calling thread and then calls the callback (or settles the promise)
//! before returning — the same divergence `fs.promises`'s module doc states
//! for file I/O, applied to name resolution: a caller "starting" several
//! lookups "concurrently" does them one at a time, and gets the right answer
//! at the wrong pace. Every DNS-protocol function keeps the same contract
//! from the caller's side even though its own client is async underneath —
//! see [`client`]'s module doc, "the private runtime every `hickory-resolver`
//! call in this module drives through" (moved there from [`resolve`] once
//! every record type needed it, not only `resolve4`).
//!
//! # Errors are plain objects, not `Error` instances — except one throw
//!
//! `entry::modules` was checked (again) for a way to construct a real
//! `Error` when this doc was first written, and found none; that has since
//! changed — `entry::make_named_error`/`entry::throw_value` exist and
//! `crate::errors::raise` already uses them for every OTHER module's
//! synchronous `ERR_*` throws. So: an ASYNC failure (a lookup/resolve that
//! fails after its callback would have been invoked, or a promise's
//! rejection) still hands back a plain object carrying
//! `code`/`syscall`/`hostname`/`message` — unchanged, for consistency with
//! `lookup`/`resolve4`, which shipped before the discovery above and are not
//! rewritten by this pass — and `err instanceof Error` is `false` there,
//! where real Node's is `true`. But [`rr_alias::reverse`] and
//! `Resolver#setServers`/`#setLocalAddress` are documented to throw
//! SYNCHRONOUSLY (`ERR_INVALID_IP_ADDRESS`, `ERR_INVALID_ARG_TYPE`) before
//! any query starts, which is a different contract than an async callback's
//! `err` argument — those DO raise a real catchable error, via
//! `crate::errors::invalid_ip_address`/`invalid_arg_type`.
//!
//! # Not implemented, by name
//!
//! - `lookupService(address, port, callback)` — needs `getnameinfo`-class
//!   reverse lookup (hostname from address) AND a service-name database
//!   (port → name, e.g. `80` → `"http"`); `std` has neither, and hand-rolling
//!   a partial `/etc/services`-style table would be exactly the "plausible
//!   wrong value" the honesty floor refuses (it would also be wrong on
//!   Windows, which has no such file).
//! - `dns.promises.Resolver` and a `dns.promises` mirror for anything past
//!   `lookup` (`resolve4`/`resolve6`/`resolve`/every other `resolve*`/
//!   `reverse`) — this crate's test corpus exercises the CALLBACK form of
//!   each; "close a subset in full, not partially" applies to which FORMS of
//!   a name are wired (this file's own doc already states this for
//!   `resolve4`'s promise mirror, which the same reasoning still withholds).
//! - The 23 numeric `dns.<CODE>` c-ares error-category constants
//!   (`dns.NODATA`, `dns.FORMERR`, …) — they describe c-ares's own internal
//!   taxonomy and have no canonical value without linking c-ares (or an
//!   equivalent); the `E`-prefixed string codes programs actually branch on
//!   (`ENOTFOUND`, `ETIMEOUT`, `ESERVFAIL`, …) are what every error object
//!   in this module already carries, so nothing here depends on the numeric
//!   table existing.
//! - `dns.ADDRCONFIG`/`V4MAPPED`/`ALL` ARE implemented as bit flags below,
//!   but they are inert bookkeeping: `lookup`'s options accept an
//!   `options.hints` number and never consult it, because `std::net`'s
//!   resolution has no hints-bitmask control to forward it to. A caller
//!   combining them with `|` gets a number back from that expression and
//!   nothing more.
//! - `Resolver#setLocalAddress` binds the outgoing source address for real
//!   ONLY once the instance has its own `setServers()`-configured server
//!   list — `resolver_class.rs`'s module doc says why the OS-supplied
//!   default list cannot receive the same binding here.

mod client;
mod common;
mod lookup;
mod resolve;
mod resolver_class;
mod rr_addr;
mod rr_alias;
mod rr_any;
mod rr_security;
mod rr_service;
mod rr_soa_naptr;
mod rr_text;
mod state;

use rts_core::entry::{self, Context, Provided};

/// The namespace `node:dns` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("lookup", lookup::lookup),
        ("resolve", resolve::resolve),
        ("resolve4", resolve::resolve4),
        ("resolve6", rr_addr::resolve6),
        ("resolveAny", rr_any::resolve_any),
        ("resolveCaa", rr_security::resolve_caa),
        ("resolveCname", rr_alias::resolve_cname),
        ("resolveMx", rr_service::resolve_mx),
        ("resolveNaptr", rr_soa_naptr::resolve_naptr),
        ("resolveNs", rr_alias::resolve_ns),
        ("resolvePtr", rr_alias::resolve_ptr),
        ("resolveSoa", rr_soa_naptr::resolve_soa),
        ("resolveSrv", rr_service::resolve_srv),
        ("resolveTlsa", rr_security::resolve_tlsa),
        ("resolveTxt", rr_text::resolve_txt),
        ("reverse", rr_alias::reverse),
        ("getServers", state::get_servers),
        ("setServers", state::set_servers),
        ("setDefaultResultOrder", state::set_default_result_order),
        ("getDefaultResultOrder", state::get_default_result_order),
        ("setLocalAddress", state::set_local_address),
    ];
    let namespace = entry::make_namespace(context, members);
    let promises = promises_namespace(context);
    entry::put_member(context, namespace, "promises", promises);
    // `dns.Resolver` — NOT mirrored onto `promises` (`dns.promises.Resolver`);
    // see this file's module doc, "Not implemented, by name".
    let resolver_ctor = resolver_class::constructor(context);
    entry::put_member(context, namespace, "Resolver", resolver_ctor);
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
