//! `dns.resolve4` — the DNS-protocol resolution path, over
//! `hickory-resolver` (vetted by `docs/reference/node/crates.md` §4.5,
//! default features only: `system-config` + `tokio`, neither of which pulls
//! `ring`/`aws-lc-rs`/`quinn`). See the module doc's "Two resolution paths"
//! for how this differs from [`super::lookup`]: this speaks the DNS wire
//! protocol to the OS's configured servers directly and never consults
//! `/etc/hosts` or its Windows equivalent, which is also why it does not
//! answer `resolve4("localhost")` the way `lookup("localhost", …)` does —
//! see "`resolve4("localhost", …)` answers whatever the configured resolver
//! answers" below.
//!
//! # Blocking from the caller's side, a runtime underneath
//!
//! Same posture the module doc states for `lookup` ("Synchronous, wearing a
//! callback"): `resolve4` resolves in-line on the calling thread. What is
//! new here is that the underlying client is async — `hickory-resolver`
//! only offers `.await`-shaped lookups — so this module owns a private
//! single-thread Tokio runtime and drives every call through
//! [`runtime`]`().block_on(...)`. That runtime and the resolver built on it
//! are never exposed past this file: nothing else in `rts-node` gains an
//! async dependency by this module having one (this is the first
//! `hickory-resolver`/`tokio` use in the crate; `net/mod.rs`'s "why
//! blocking, not tokio" note is now true of `node:net` specifically, not
//! the crate as a whole).
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

use super::common::error_object;
use hickory_resolver::TokioResolver;
use hickory_resolver::net::NetError;
use hickory_resolver::proto::rr::RData;
use rts_core::entry;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

/// The private runtime every `hickory-resolver` call in this module drives
/// through. Current-thread: nothing here runs concurrently with itself, and
/// a caller inside `resolve4` already blocks until the one query settles.
fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("building a current-thread Tokio runtime for node:dns resolve4")
    })
}

/// The process-wide resolver, built once from the OS's own configuration
/// (`/etc/resolv.conf` on Unix, the resolver the Windows registry names —
/// `hickory-resolver`'s `system-config` feature, per the module doc) the
/// first time `resolve4` is called. A build failure (no usable system
/// configuration found) is cached and reported to every subsequent caller
/// rather than retried silently.
fn resolver() -> Result<&'static TokioResolver, NetError> {
    static RESOLVER: OnceLock<Result<TokioResolver, NetError>> = OnceLock::new();
    RESOLVER
        .get_or_init(|| runtime().block_on(async { TokioResolver::builder_tokio()?.build() }))
        .as_ref()
        .map_err(Clone::clone)
}

/// The Node error code closest to what `err` represents.
/// `ENOTFOUND` for "the name does not exist"/"no A records exist for it" —
/// the two cases a program's `err.code === 'ENOTFOUND'` check means to
/// catch — `ETIMEOUT` for a query that never got an answer, and
/// `ESERVFAIL` as the named fallback for every other protocol failure
/// (malformed response, refused query, transport error), matching what
/// `queryA` reports in real Node when the failure is not one of the first
/// two shapes.
fn node_code(err: &NetError) -> &'static str {
    if err.is_nx_domain() || err.is_no_records_found() {
        "ENOTFOUND"
    } else if matches!(err, NetError::Timeout) {
        "ETIMEOUT"
    } else {
        "ESERVFAIL"
    }
}

/// `dns.resolve4(hostname, options?, callback)`.
///
/// Same last-argument overload `dns.lookup` uses: the 3rd slot present means
/// the 2nd slot is `options` (`{ ttl: boolean }`), otherwise the 2nd slot is
/// the callback and there are no options.
pub(super) extern "C" fn resolve4(_e: u64, _this: u64, hostname: u64, arg1: u64, arg2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let (options, callback) = match arg2 == absent {
        true => (absent, arg1),
        false => (arg1, arg2),
    };
    if callback == absent {
        return absent;
    }
    let Some(host) = entry::text_of(hostname) else {
        let error = error_object("ERR_INVALID_ARG_TYPE", "queryA", "");
        entry::call(callback, absent, error, absent, absent, absent);
        return absent;
    };
    let with_ttl = super::common::option_bool(options, "ttl");

    let resolver = match resolver() {
        Ok(resolver) => resolver,
        Err(err) => {
            let error = error_object(node_code(&err), "queryA", &host);
            return entry::call(callback, absent, error, absent, absent, absent);
        }
    };
    match runtime().block_on(resolver.ipv4_lookup(host.as_str())) {
        Ok(lookup) => {
            let null = entry::null_value();
            let array = ipv4_array(lookup.answers(), with_ttl);
            entry::call(callback, absent, null, array, absent, absent)
        }
        Err(err) => {
            let error = error_object(node_code(&err), "queryA", &host);
            entry::call(callback, absent, error, absent, absent, absent)
        }
    }
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
fn ipv4_array(answers: &[hickory_resolver::proto::rr::Record], with_ttl: bool) -> u64 {
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
