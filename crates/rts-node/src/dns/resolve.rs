//! `dns.resolve4` and the generic `dns.resolve` dispatcher — the
//! DNS-protocol resolution path, over `hickory-resolver` (vetted by
//! `docs/reference/node/crates.md` §4.5). See the module doc's "Two
//! resolution paths" for how this differs from [`super::lookup`]: this
//! speaks the DNS wire protocol to the OS's configured servers directly and
//! never consults `/etc/hosts` or its Windows equivalent, which is also why
//! it does not answer `resolve4("localhost")` the way `lookup("localhost", …)`
//! does — see "`resolve4("localhost", …)` answers whatever the configured
//! resolver answers" below.
//!
//! The shared client machinery (the private Tokio runtime, the process-wide
//! resolver singleton, the `NetError` → Node-code mapping) moved to
//! [`super::client`] once every other record type joined `resolve4` here —
//! see that module's doc for "Blocking from the caller's side, a runtime
//! underneath", which was this file's own doc until then.
//!
//! # `resolve4("localhost", …)` answers whatever the configured resolver answers
//!
//! A `resolve4` faithful to what Node's `resolve4` actually does (DNS
//! protocol only, no hosts file, no `getaddrinfo`) does not special-case
//! `"localhost"` — real Node does not either (`nodejs/help#2163` is the
//! public report of exactly this: `dns.resolve4('localhost', …)` can fail
//! where `dns.lookup('localhost', …)` succeeds). What it answers depends on
//! what the OS's configured DNS server does with an `A` query for
//! `localhost`: queried directly, a bare public resolver (measured here
//! against `8.8.8.8` with `nslookup -type=A localhost 8.8.8.8`) answers
//! NXDOMAIN, because `localhost` is not a real zone any authoritative
//! server serves. A local recursive/filtering resolver (a machine's actual
//! configured server, which `system-config` reads and is not necessarily
//! the same one a manual probe names explicitly) is free to special-case it
//! per RFC 6761 §6.3 before ever forwarding the query, the same way many
//! stub resolvers do — and on the machine this was built and tested on, the
//! configured resolver does exactly that: `resolve4("localhost", cb)`
//! measured a real positive answer end to end (`target/…rts.exe test
//! tests/node_dns_full.test.ts`), not a fabricated one. Both measurements
//! are real; they are not in tension, because they asked two different
//! servers. Whichever way this machine's configured resolver answers,
//! `resolve4` reports it as measured — special-casing `"localhost"` inside
//! `resolve4` to hand back a synthetic `127.0.0.1` regardless of what the
//! configured resolver says would be exactly the fabricated answer the
//! honesty floor refuses.

use super::client::{node_code, resolver, runtime};
use super::common::callback_with_ttl_option;
use hickory_resolver::TokioResolver;
use hickory_resolver::proto::rr::{RData, Record};
use rts_core::entry;

/// `dns.resolve4(hostname, options?, callback)`.
///
/// Same last-argument overload `dns.lookup` uses: the 3rd slot present means
/// the 2nd slot is `options` (`{ ttl: boolean }`), otherwise the 2nd slot is
/// the callback and there are no options.
pub(super) extern "C" fn resolve4(_e: u64, _this: u64, hostname: u64, arg1: u64, arg2: u64, _a3: u64) -> u64 {
    callback_with_ttl_option(hostname, arg1, arg2, "queryA", |host, with_ttl| {
        let resolver = resolver().map_err(|err| node_code(&err))?;
        value(resolver, host, with_ttl)
    })
}

/// The query + decode `resolve4` shares with `dns.Resolver#resolve4` and
/// `dns.resolve(host, 'A', cb)` (the default `rrtype`) — see
/// [`resolve`] below.
pub(super) fn value(resolver: &TokioResolver, host: &str, with_ttl: bool) -> Result<u64, &'static str> {
    let lookup = runtime().block_on(resolver.ipv4_lookup(host)).map_err(|err| node_code(&err))?;
    Ok(ipv4_array(lookup.answers(), with_ttl))
}

/// The `A` records among `answers` as a JS array — plain `"a.b.c.d"`
/// strings, or `{ address, ttl }` objects when `options.ttl === true`. A
/// non-`A` answer (e.g. a `CNAME` the server chased for us) is skipped
/// rather than stringified, matching what real Node's `resolve4` reports.
///
/// `record.data`/`record.ttl` are read as FIELDS, not `.data()`/`.ttl()`
/// calls: `hickory_proto::rr::Record` exposes those only as public fields —
/// the `.data()`/`.ttl()` *methods* of the same name belong to the sibling
/// zero-copy `RecordRef` type this crate never holds one of.
fn ipv4_array(answers: &[Record], with_ttl: bool) -> u64 {
    entry::with_runtime(|context| {
        let items: Vec<u64> = answers
            .iter()
            .filter_map(|record| match &record.data {
                RData::A(address) => Some((address.0.to_string(), record.ttl)),
                _ => None,
            })
            .map(|(address, ttl)| {
                if with_ttl {
                    let object = entry::make_object(context);
                    let address_v = entry::make_string(context, &address);
                    entry::put_member(context, object, "address", address_v);
                    let ttl_v = entry::make_number(f64::from(ttl));
                    entry::put_member(context, object, "ttl", ttl_v);
                    object
                } else {
                    entry::make_string(context, &address)
                }
            })
            .collect();
        entry::make_array_in(context, items)
    })
}

/// `dns.resolve(hostname[, rrtype], callback)` — dispatches to the same
/// query-and-decode every dedicated `resolve<Type>` native runs, keyed by
/// the eleven strings `docs/reference/node/dns.md` §2 documents
/// (`rrtype` defaults to `'A'`, matching Node). An unrecognized `rrtype`
/// answers via the callback with `ERR_INVALID_ARG_VALUE` rather than
/// silently falling back to `'A'` — Node throws synchronously for this case
/// (a `TypeError` naming the allowed values); this crate cannot raise THAT
/// specific catchable error from a value it hasn't looked at until the
/// query is otherwise underway, so the callback path is the honest
/// approximation rather than a silent wrong answer.
pub(super) extern "C" fn resolve(_e: u64, _this: u64, hostname: u64, arg1: u64, arg2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let (rrtype, callback) = match arg2 == absent {
        true => (absent, arg1),
        false => (arg1, arg2),
    };
    let kind = entry::text_of(rrtype).unwrap_or_else(|| "A".to_owned());
    super::common::callback_result(hostname, callback, syscall_for(&kind), |host| {
        let resolver = resolver().map_err(|err| node_code(&err))?;
        dispatch(resolver, host, &kind)
    })
}

/// The `syscall` field Node's own dispatcher would report for `rrtype` — the
/// same `query<Type>` spelling each dedicated native already uses. `pub(super)`
/// rather than private: `dns.Resolver#resolve` (`resolver_class.rs`) is the
/// same dispatch keyed to its own resolver instead of the process-wide one.
pub(super) fn syscall_for(kind: &str) -> &'static str {
    match kind {
        "AAAA" => "queryAaaa",
        "ANY" => "resolveAny",
        "CAA" => "queryCaa",
        "CNAME" => "queryCname",
        "MX" => "queryMx",
        "NAPTR" => "queryNaptr",
        "NS" => "queryNs",
        "PTR" => "queryPtr",
        "SOA" => "querySoa",
        "SRV" => "querySrv",
        "TLSA" => "queryTlsa",
        "TXT" => "queryTxt",
        _ => "queryA",
    }
}

/// `pub(super)` for the same reason [`syscall_for`] is — shared with
/// `dns.Resolver#resolve`.
pub(super) fn dispatch(resolver: &TokioResolver, host: &str, kind: &str) -> Result<u64, &'static str> {
    match kind {
        "A" => value(resolver, host, false),
        "AAAA" => super::rr_addr::value(resolver, host, false),
        "ANY" => super::rr_any::value(resolver, host),
        "CAA" => super::rr_security::caa_value(resolver, host),
        "CNAME" => super::rr_alias::cname_value(resolver, host),
        "MX" => super::rr_service::mx_value(resolver, host),
        "NAPTR" => super::rr_soa_naptr::naptr_value(resolver, host),
        "NS" => super::rr_alias::ns_value(resolver, host),
        "PTR" => super::rr_alias::ptr_value(resolver, host),
        "SOA" => super::rr_soa_naptr::soa_value(resolver, host),
        "SRV" => super::rr_service::srv_value(resolver, host),
        "TLSA" => super::rr_security::tlsa_value(resolver, host),
        "TXT" => super::rr_text::value(resolver, host),
        _ => Err("ERR_INVALID_ARG_VALUE"),
    }
}
