//! `dns.resolve6` — AAAA records, the same result shape [`super::resolve`]'s
//! `resolve4` answers for A (plain address strings, or `{address, ttl}` when
//! `options.ttl === true`), over the same DNS-protocol client. Kept in its
//! own file, beside the rest of the record-type decoders that joined this
//! crate alongside it, to stay under the file-size ceiling `resolve.rs`
//! would otherwise cross.

use super::client::{node_code, resolver, runtime};
use super::common::callback_with_ttl_option;
use hickory_resolver::TokioResolver;
use hickory_resolver::proto::rr::{RData, Record};
use rts_core::entry;

/// `dns.resolve6(hostname[, options], callback)`.
pub(super) extern "C" fn resolve6(_e: u64, _this: u64, hostname: u64, arg1: u64, arg2: u64, _a3: u64) -> u64 {
    callback_with_ttl_option(hostname, arg1, arg2, "queryAaaa", |host, with_ttl| {
        let resolver = resolver().map_err(|err| node_code(&err))?;
        value(resolver, host, with_ttl)
    })
}

/// The query + decode `resolve6` shares with `dns.Resolver#resolve6` and
/// `dns.resolve(host, 'AAAA', cb)` — see [`super::resolve::resolve`].
pub(super) fn value(resolver: &TokioResolver, host: &str, with_ttl: bool) -> Result<u64, &'static str> {
    let lookup = runtime().block_on(resolver.ipv6_lookup(host)).map_err(|err| node_code(&err))?;
    Ok(array(lookup.answers(), with_ttl))
}

/// The `AAAA` records among `answers` as a JS array — plain `"::1"`-shaped
/// strings, or `{ address, ttl }` objects when `with_ttl`. A non-`AAAA`
/// answer is skipped rather than stringified, matching what `resolve.rs`'s
/// `ipv4_array` already does for A.
fn array(answers: &[Record], with_ttl: bool) -> u64 {
    entry::with_runtime(|context| {
        let items: Vec<u64> = answers
            .iter()
            .filter_map(|record| match &record.data {
                RData::AAAA(address) => Some((address.0.to_string(), record.ttl)),
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
