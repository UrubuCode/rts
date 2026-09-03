//! `dns.resolveCaa`/`resolveTlsa` — the two record types whose RDATA is
//! mostly opaque bytes rather than a `Name` or a small integer: CAA's
//! `value` (interpreted only far enough to know which named field of
//! `CaaRecord` to put it under) and TLSA's certificate-association `data`
//! (kept as raw bytes, not interpreted at all — this crate has no
//! certificate-parsing dependency and Node's own `resolveTlsa` does not
//! parse it either).

use super::client::{node_code, resolver, runtime};
use super::common::callback_result;
use hickory_resolver::TokioResolver;
use hickory_resolver::proto::rr::RData;
use rts_core::entry;

/// `dns.resolveCaa(hostname, callback)` → `CaaRecord[]`.
pub(super) extern "C" fn resolve_caa(_e: u64, _this: u64, hostname: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    callback_result(hostname, callback, "queryCaa", |host| {
        let resolver = resolver().map_err(|err| node_code(&err))?;
        caa_value(resolver, host)
    })
}

struct Caa {
    /// The raw wire flags byte — `caa.flags()` combines the issuer-critical
    /// bit with `reserved_flags`, which is exactly what Node's own `critical`
    /// field carries: `128` (`0x80`, the critical bit alone) or `0`, NOT a
    /// `0`/`1` boolean-shaped number — measured against real Node's own docs
    /// example, `{critical: 0, iodef: '…'}, {critical: 128, issue: '…'}`.
    critical: u8,
    tag: String,
    value: String,
}

pub(super) fn caa_value(resolver: &TokioResolver, host: &str) -> Result<u64, &'static str> {
    let lookup = runtime().block_on(resolver.lookup(host, hickory_resolver::proto::rr::RecordType::CAA)).map_err(|err| node_code(&err))?;
    let records: Vec<Caa> = lookup
        .answers()
        .iter()
        .filter_map(|record| match &record.data {
            RData::CAA(caa) => Some(Caa {
                critical: caa.flags(),
                tag: caa.tag.clone(),
                value: String::from_utf8_lossy(&caa.value).into_owned(),
            }),
            _ => None,
        })
        .collect();
    Ok(entry::with_runtime(|context| {
        let mut items = Vec::with_capacity(records.len());
        for record in &records {
            let object = entry::make_object(context);
            let critical_v = entry::make_number(f64::from(record.critical));
            entry::put_member(context, object, "critical", critical_v);
            // The property is NAMED by the record's own tag — `issue`,
            // `issuewild`, `iodef`, or (RFC 8659 allows others)
            // `contactemail`/`contactphone`/whatever the server sent, put
            // under that literal name so a caller reading `record.issue`
            // for an issue-tagged record still gets it, without this crate
            // pretending to enumerate every tag Node's docs happen to list.
            let value_v = entry::make_string(context, &record.value);
            entry::put_member(context, object, &record.tag, value_v);
            items.push(object);
        }
        entry::make_array_in(context, items)
    }))
}

/// `dns.resolveTlsa(hostname, callback)` → `TlsaRecord[]`.
pub(super) extern "C" fn resolve_tlsa(_e: u64, _this: u64, hostname: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    callback_result(hostname, callback, "queryTlsa", |host| {
        let resolver = resolver().map_err(|err| node_code(&err))?;
        tlsa_value(resolver, host)
    })
}

struct Tlsa {
    cert_usage: u8,
    selector: u8,
    matching: u8,
    data: Vec<u8>,
}

pub(super) fn tlsa_value(resolver: &TokioResolver, host: &str) -> Result<u64, &'static str> {
    let lookup = runtime().block_on(resolver.tlsa_lookup(host)).map_err(|err| node_code(&err))?;
    let records: Vec<Tlsa> = lookup
        .answers()
        .iter()
        .filter_map(|record| match &record.data {
            RData::TLSA(tlsa) => Some(Tlsa {
                cert_usage: u8::from(tlsa.cert_usage.clone()),
                selector: u8::from(tlsa.selector.clone()),
                matching: u8::from(tlsa.matching.clone()),
                data: tlsa.cert_data.clone(),
            }),
            _ => None,
        })
        .collect();
    Ok(entry::with_runtime(|context| {
        let mut items = Vec::with_capacity(records.len());
        for record in &records {
            let object = entry::make_object(context);
            let cert_usage_v = entry::make_number(f64::from(record.cert_usage));
            entry::put_member(context, object, "certUsage", cert_usage_v);
            let selector_v = entry::make_number(f64::from(record.selector));
            entry::put_member(context, object, "selector", selector_v);
            let matching_v = entry::make_number(f64::from(record.matching));
            entry::put_member(context, object, "match", matching_v);
            // A `Buffer` (extends `Uint8Array`), not the `ArrayBuffer` the
            // .d.ts names — the same divergence every OTHER binary-data
            // answer in this crate takes (`hash.digest()` with no encoding,
            // say), since a `Buffer` is a strict superset of what an
            // `ArrayBuffer` offers a caller here.
            let data_v = entry::make_buffer(context, &record.data);
            entry::put_member(context, object, "data", data_v);
            items.push(object);
        }
        entry::make_array_in(context, items)
    }))
}
