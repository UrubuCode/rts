//! `dns.resolveTxt` — `string[][]`: one inner array PER TXT record, its
//! entries the record's own wire-level chunks (a single TXT record's value is
//! a sequence of `<character-string>`s, each up to 255 bytes, and Node keeps
//! them separate rather than concatenating — see `docs/reference/node/dns.md`
//! §2, "one inner array per TXT record; multiple chunks per record are
//! individual strings").

use super::client::{node_code, resolver, runtime};
use super::common::callback_result;
use hickory_resolver::TokioResolver;
use hickory_resolver::proto::rr::RData;
use rts_core::entry;

/// `dns.resolveTxt(hostname, callback)`.
pub(super) extern "C" fn resolve_txt(_e: u64, _this: u64, hostname: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    callback_result(hostname, callback, "queryTxt", |host| {
        let resolver = resolver().map_err(|err| node_code(&err))?;
        value(resolver, host)
    })
}

/// The query + decode `resolveTxt` shares with `dns.Resolver#resolveTxt` and
/// `dns.resolve(host, 'TXT', cb)`.
pub(super) fn value(resolver: &TokioResolver, host: &str) -> Result<u64, &'static str> {
    let lookup = runtime().block_on(resolver.txt_lookup(host)).map_err(|err| node_code(&err))?;
    // Collected into plain Rust `String`s first, and the JS array-of-arrays
    // built from those with plain `for` loops rather than nested `.map`
    // closures — two closures sharing one `&mut Context` reborrow is legal
    // Rust, but an imperative loop says the same thing without asking a
    // reader to work that out.
    let records: Vec<Vec<String>> = lookup
        .answers()
        .iter()
        .filter_map(|record| match &record.data {
            RData::TXT(txt) => Some(txt),
            _ => None,
        })
        .map(|txt| txt.txt_data.iter().map(|chunk| String::from_utf8_lossy(chunk).into_owned()).collect())
        .collect();
    Ok(entry::with_runtime(|context| {
        let mut record_values = Vec::with_capacity(records.len());
        for chunks in &records {
            let mut chunk_values = Vec::with_capacity(chunks.len());
            for chunk in chunks {
                chunk_values.push(entry::make_string(context, chunk));
            }
            record_values.push(entry::make_array_in(context, chunk_values));
        }
        entry::make_array_in(context, record_values)
    }))
}
