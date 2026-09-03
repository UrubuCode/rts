//! `dns.Resolver` — an independently-configured DNS resolver instance. Real
//! Node's whole point of this class is that its `setServers`/
//! `setLocalAddress`/`timeout`/`tries` do NOT touch the global config
//! `state.rs` holds, and DO reach the servers it actually queries — see that
//! module's doc for why the module-level `dns.setServers()` is already inert
//! against `resolve4`. A `Resolver` whose own setters were equally inert
//! would be exactly the "surface that cannot do what its name means" this
//! crate's rules refuse, so every method here builds (or reuses) a REAL
//! per-instance `hickory-resolver` client via [`super::client::build_resolver`]
//! rather than the process-wide singleton [`super::client::resolver`] every
//! module-level native queries through.
//!
//! # Where the state lives
//!
//! Not on the JS instance — a live `TokioResolver` (or the small config that
//! builds one) is native state no value in this engine's shape-and-property
//! system can hold, the same limit `crypto/hash.rs`'s module doc states for a
//! mid-update digest. So an instance carries one hidden `__resolverId`
//! number, and [`TABLE`] is where the configuration actually lives, keyed the
//! same generation-free way `crypto/hash.rs`/`string_decoder.rs` key their
//! own native state.
//!
//! # A resolver is rebuilt on every call, not cached
//!
//! `hickory-resolver`'s `Resolver` has no `setServers`-after-the-fact: its
//! server list is fixed at `.build()`. Caching one and invalidating it on
//! every `setServers()`/`setLocalAddress()` call would need to notice every
//! path that writes [`ResolverState`]; rebuilding fresh from the stored
//! config at the START of each `resolve*` call instead means there is
//! nothing to keep in sync — the cost is one extra (network-free, config-only)
//! build per query, which is not the part of a DNS round trip that is slow.
//!
//! # Why `cancel()` is an honest no-op here
//!
//! Real Node's `cancel()` fails every OUTSTANDING query on that resolver with
//! `ECANCELLED`. This module has no outstanding query for it to find: the
//! crate's synchronous-from-the-caller's-side contract (`mod.rs`'s "Synchronous,
//! wearing a callback") means `resolver.resolve4(host, cb)` has already
//! called `cb` — and every native here blocks the ONE JS thread until its
//! query settles — by the time ANY other JS statement, `cancel()` included,
//! can run. So this never finds a query to cancel; answering `undefined` and
//! touching nothing is the true state of the world, not a paved-over gap.
//!
//! # Not implemented, by name
//!
//! - `dns.promises.Resolver` — the promise mirror of this class. `mod.rs`'s
//!   `promises_namespace` already withholds `resolve4` from `dns.promises`
//!   for the same reason: this crate's test corpus does not exercise the
//!   promise FORM of any resolver call, and closing a subset in full means
//!   closing the forms something exercises, not every form a name has.
//! - `setLocalAddress` binding when the instance has never called
//!   `setServers()` — see [`super::client::build_resolver`]'s doc for why: the
//!   OS-supplied server list is an opaque already-built config this crate
//!   does not re-derive a per-server connection from.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use hickory_resolver::TokioResolver;
use rts_core::entry::{self, Context, Provided};

use super::client::{build_resolver, node_code};
use super::common::{callback_result, callback_with_ttl_option, error_object, option_number, parse_server_addr};

struct ResolverState {
    servers: Vec<String>,
    timeout_ms: Option<u32>,
    tries: Option<u32>,
    local_v4: Option<IpAddr>,
    local_v6: Option<IpAddr>,
}

static TABLE: Mutex<Option<HashMap<u64, ResolverState>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_table<T>(body: impl FnOnce(&mut HashMap<u64, ResolverState>) -> T) -> T {
    let mut guard = TABLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

const METHODS: &[(&str, Provided)] = &[
    ("resolve", resolve),
    ("resolve4", resolve4),
    ("resolve6", resolve6),
    ("resolveAny", resolve_any),
    ("resolveCaa", resolve_caa),
    ("resolveCname", resolve_cname),
    ("resolveMx", resolve_mx),
    ("resolveNaptr", resolve_naptr),
    ("resolveNs", resolve_ns),
    ("resolvePtr", resolve_ptr),
    ("resolveSoa", resolve_soa),
    ("resolveSrv", resolve_srv),
    ("resolveTlsa", resolve_tlsa),
    ("resolveTxt", resolve_txt),
    ("reverse", reverse),
    ("getServers", get_servers),
    ("setServers", set_servers),
    ("setLocalAddress", set_local_address),
    ("cancel", cancel),
];

pub(super) fn prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "Resolver", METHODS)
}

/// The `Resolver` constructor, `.prototype` already attached — ready to hang
/// on `dns`'s namespace as `dns.Resolver`. The same `ctor.prototype = …`
/// shape `net/mod.rs`'s `class_ctor` builds; kept local rather than shared
/// across module boundaries, the crate's usual posture for a helper this
/// small (`net/common.rs::self_or_new`'s doc names the same choice).
pub(super) fn constructor(context: &mut Context) -> u64 {
    let ctor = entry::make_callable(context, construct);
    let proto = prototype(context);
    entry::put_member(context, ctor, "prototype", proto);
    ctor
}

/// `new dns.Resolver(options?)` — `{ timeout, tries, maxTimeout }`.
/// `maxTimeout` is read nowhere below: `hickory-resolver`'s `ResolverOpts`
/// has a single `timeout` (this crate maps `options.timeout` onto it) and no
/// second "cap between retries" knob to receive it, the same "accepted but
/// inert" posture `dns.ADDRCONFIG`/`V4MAPPED`/`ALL` already have on
/// `lookup`'s `hints`.
pub(super) extern "C" fn construct(_e: u64, this: u64, options: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let timeout_ms = option_number(options, "timeout").filter(|value| *value >= 0.0).map(|value| value as u32);
    let tries = option_number(options, "tries").map(|value| value as u32);
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    with_table(|table| {
        table.insert(
            id,
            ResolverState { servers: Vec::new(), timeout_ms, tries, local_v4: None, local_v6: None },
        );
    });
    entry::with_runtime(|context| {
        let prototype = prototype(context);
        let instance = match entry::is_object(context, this) {
            true => this,
            false => entry::make_instance(context, prototype),
        };
        let id_v = entry::make_number(id as f64);
        entry::put_member(context, instance, "__resolverId", id_v);
        instance
    })
}

fn instance_id(this: u64) -> Option<u64> {
    entry::with_runtime(|context| entry::number_of(entry::get_member(context, this, "__resolverId"))).map(|value| value as u64)
}

/// A fresh `TokioResolver` built from `this` instance's own configuration —
/// see the module doc, "A resolver is rebuilt on every call, not cached".
fn resolver_for(this: u64) -> Result<TokioResolver, &'static str> {
    let id = instance_id(this).ok_or("ERR_INVALID_ARG_TYPE")?;
    let config = with_table(|table| {
        table.get(&id).map(|state| (state.servers.clone(), state.timeout_ms, state.tries, state.local_v4, state.local_v6))
    });
    let (servers, timeout_ms, tries, local_v4, local_v6) = config.ok_or("ERR_INVALID_ARG_TYPE")?;
    build_resolver(&servers, timeout_ms, tries, local_v4, local_v6).map_err(|err| node_code(&err))
}

extern "C" fn resolve4(_e: u64, this: u64, hostname: u64, arg1: u64, arg2: u64, _a3: u64) -> u64 {
    callback_with_ttl_option(hostname, arg1, arg2, "queryA", |host, with_ttl| super::resolve::value(&resolver_for(this)?, host, with_ttl))
}

extern "C" fn resolve6(_e: u64, this: u64, hostname: u64, arg1: u64, arg2: u64, _a3: u64) -> u64 {
    callback_with_ttl_option(hostname, arg1, arg2, "queryAaaa", |host, with_ttl| super::rr_addr::value(&resolver_for(this)?, host, with_ttl))
}

extern "C" fn resolve_cname(_e: u64, this: u64, hostname: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    callback_result(hostname, callback, "queryCname", |host| super::rr_alias::cname_value(&resolver_for(this)?, host))
}

extern "C" fn resolve_ns(_e: u64, this: u64, hostname: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    callback_result(hostname, callback, "queryNs", |host| super::rr_alias::ns_value(&resolver_for(this)?, host))
}

extern "C" fn resolve_ptr(_e: u64, this: u64, hostname: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    callback_result(hostname, callback, "queryPtr", |host| super::rr_alias::ptr_value(&resolver_for(this)?, host))
}

extern "C" fn resolve_mx(_e: u64, this: u64, hostname: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    callback_result(hostname, callback, "queryMx", |host| super::rr_service::mx_value(&resolver_for(this)?, host))
}

extern "C" fn resolve_srv(_e: u64, this: u64, hostname: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    callback_result(hostname, callback, "querySrv", |host| super::rr_service::srv_value(&resolver_for(this)?, host))
}

extern "C" fn resolve_soa(_e: u64, this: u64, hostname: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    callback_result(hostname, callback, "querySoa", |host| super::rr_soa_naptr::soa_value(&resolver_for(this)?, host))
}

extern "C" fn resolve_naptr(_e: u64, this: u64, hostname: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    callback_result(hostname, callback, "queryNaptr", |host| super::rr_soa_naptr::naptr_value(&resolver_for(this)?, host))
}

extern "C" fn resolve_caa(_e: u64, this: u64, hostname: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    callback_result(hostname, callback, "queryCaa", |host| super::rr_security::caa_value(&resolver_for(this)?, host))
}

extern "C" fn resolve_tlsa(_e: u64, this: u64, hostname: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    callback_result(hostname, callback, "queryTlsa", |host| super::rr_security::tlsa_value(&resolver_for(this)?, host))
}

extern "C" fn resolve_txt(_e: u64, this: u64, hostname: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    callback_result(hostname, callback, "queryTxt", |host| super::rr_text::value(&resolver_for(this)?, host))
}

extern "C" fn resolve_any(_e: u64, this: u64, hostname: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    callback_result(hostname, callback, "resolveAny", |host| super::rr_any::value(&resolver_for(this)?, host))
}

/// `resolver.resolve(hostname[, rrtype], callback)` — the same dispatch
/// [`super::resolve::resolve`] runs, over `this` instance's own resolver.
extern "C" fn resolve(_e: u64, this: u64, hostname: u64, arg1: u64, arg2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let (rrtype, callback) = match arg2 == absent {
        true => (absent, arg1),
        false => (arg1, arg2),
    };
    let kind = entry::text_of(rrtype).unwrap_or_else(|| "A".to_owned());
    callback_result(hostname, callback, super::resolve::syscall_for(&kind), |host| super::resolve::dispatch(&resolver_for(this)?, host, &kind))
}

/// `resolver.reverse(ip, callback)` — throws synchronously on a malformed
/// `ip`, the same contract [`super::rr_alias::reverse`] documents.
extern "C" fn reverse(_e: u64, this: u64, ip: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let Some(text) = entry::text_of(ip) else {
        crate::errors::invalid_ip_address("");
        return absent;
    };
    let Ok(address) = text.parse::<IpAddr>() else {
        crate::errors::invalid_ip_address(&text);
        return absent;
    };
    if callback == absent {
        return absent;
    }
    let outcome = resolver_for(this).and_then(|resolver| super::rr_alias::reverse_value(&resolver, address));
    match outcome {
        Ok(value) => {
            let null = entry::null_value();
            entry::call(callback, absent, null, value, absent, absent)
        }
        Err(code) => {
            let error = error_object(code, "queryPtr", &text);
            entry::call(callback, absent, error, absent, absent, absent)
        }
    }
}

/// `resolver.getServers()` — THIS instance's own list, independent of the
/// module-level `dns.getServers()`.
extern "C" fn get_servers(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let servers = instance_id(this).and_then(|id| with_table(|table| table.get(&id).map(|state| state.servers.clone()))).unwrap_or_default();
    entry::with_runtime(|context| {
        let items: Vec<u64> = servers.iter().map(|s| entry::make_string(context, s)).collect();
        entry::make_array_in(context, items)
    })
}

/// `resolver.setServers(servers)`. Unlike the module-level `dns.setServers`
/// (`state.rs`, which cannot raise), this DOES throw synchronously on a
/// malformed entry — the contract `docs/reference/node/dns.md` §2 documents
/// — because `crate::errors::invalid_ip_address` reaches a real catchable
/// error this module doc's own history did not know was available when
/// `state.rs` was first written.
extern "C" fn set_servers(_e: u64, this: u64, servers: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let Some(entries) = super::state::array_texts(servers) else {
        crate::errors::invalid_arg_type("servers", "Array", servers);
        return absent;
    };
    for item in &entries {
        if parse_server_addr(item).is_none() {
            crate::errors::invalid_ip_address(item);
            return absent;
        }
    }
    let Some(id) = instance_id(this) else { return absent };
    with_table(|table| {
        if let Some(state) = table.get_mut(&id) {
            state.servers = entries;
        }
    });
    absent
}

/// `resolver.setLocalAddress(ipv4?, ipv6?)` — see the module doc's "Not
/// implemented, by name" for the one case this stays inert in.
extern "C" fn set_local_address(_e: u64, this: u64, ipv4: u64, ipv6: u64, _a2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let ipv4_text = entry::text_of(ipv4);
    let ipv6_text = entry::text_of(ipv6);
    for text in ipv4_text.iter().chain(ipv6_text.iter()) {
        if text.parse::<IpAddr>().is_err() {
            crate::errors::invalid_ip_address(text);
            return absent;
        }
    }
    let Some(id) = instance_id(this) else { return absent };
    with_table(|table| {
        let Some(state) = table.get_mut(&id) else { return };
        if let Some(text) = ipv4_text {
            state.local_v4 = text.parse().ok();
        }
        if let Some(text) = ipv6_text {
            state.local_v6 = text.parse().ok();
        }
    });
    absent
}

/// `resolver.cancel()` — see the module doc, "Why `cancel()` is an honest
/// no-op here".
extern "C" fn cancel(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::undefined_value()
}
