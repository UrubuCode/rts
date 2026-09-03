//! `dns.resolveSoa`/`resolveNaptr` — the zone-authority record (a single
//! object, not an array — a zone has exactly one SOA) and the DDDS-algorithm
//! record (an array of objects, each carrying three raw character-string
//! fields alongside its two `Name`-derived and two numeric ones).

use super::client::{node_code, resolver, runtime};
use super::common::callback_result;
use hickory_resolver::TokioResolver;
use hickory_resolver::proto::rr::RData;
use rts_core::entry;

/// `dns.resolveSoa(hostname, callback)` → a single `SoaRecord` object, NOT
/// an array — see this file's module doc. A zone answering zero SOA records
/// (the server chased something else entirely) is `ENOTFOUND`, the same code
/// every other empty-answer case in this crate's DNS-protocol path uses.
pub(super) extern "C" fn resolve_soa(_e: u64, _this: u64, hostname: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    callback_result(hostname, callback, "querySoa", |host| {
        let resolver = resolver().map_err(|err| node_code(&err))?;
        soa_value(resolver, host)
    })
}

pub(super) fn soa_value(resolver: &TokioResolver, host: &str) -> Result<u64, &'static str> {
    let lookup = runtime().block_on(resolver.soa_lookup(host)).map_err(|err| node_code(&err))?;
    let soa = lookup
        .answers()
        .iter()
        .find_map(|record| match &record.data {
            RData::SOA(soa) => Some(soa.clone()),
            _ => None,
        })
        .ok_or("ENOTFOUND")?;
    let mname = soa.mname.to_utf8().trim_end_matches('.').to_owned();
    let rname = soa.rname.to_utf8().trim_end_matches('.').to_owned();
    Ok(entry::with_runtime(|context| {
        let object = entry::make_object(context);
        let mname_v = entry::make_string(context, &mname);
        entry::put_member(context, object, "nsname", mname_v);
        let rname_v = entry::make_string(context, &rname);
        entry::put_member(context, object, "hostmaster", rname_v);
        let serial_v = entry::make_number(f64::from(soa.serial));
        entry::put_member(context, object, "serial", serial_v);
        let refresh_v = entry::make_number(f64::from(soa.refresh));
        entry::put_member(context, object, "refresh", refresh_v);
        let retry_v = entry::make_number(f64::from(soa.retry));
        entry::put_member(context, object, "retry", retry_v);
        let expire_v = entry::make_number(f64::from(soa.expire));
        entry::put_member(context, object, "expire", expire_v);
        let minttl_v = entry::make_number(f64::from(soa.minimum));
        entry::put_member(context, object, "minttl", minttl_v);
        object
    }))
}

/// `dns.resolveNaptr(hostname, callback)` → `NaptrRecord[]`.
pub(super) extern "C" fn resolve_naptr(_e: u64, _this: u64, hostname: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    callback_result(hostname, callback, "queryNaptr", |host| {
        let resolver = resolver().map_err(|err| node_code(&err))?;
        naptr_value(resolver, host)
    })
}

struct Naptr {
    order: u16,
    preference: u16,
    flags: String,
    services: String,
    regexp: String,
    replacement: String,
}

pub(super) fn naptr_value(resolver: &TokioResolver, host: &str) -> Result<u64, &'static str> {
    let lookup = runtime().block_on(resolver.lookup(host, hickory_resolver::proto::rr::RecordType::NAPTR)).map_err(|err| node_code(&err))?;
    let records: Vec<Naptr> = lookup
        .answers()
        .iter()
        .filter_map(|record| match &record.data {
            RData::NAPTR(naptr) => Some(Naptr {
                order: naptr.order,
                preference: naptr.preference,
                flags: String::from_utf8_lossy(&naptr.flags).into_owned(),
                services: String::from_utf8_lossy(&naptr.services).into_owned(),
                regexp: String::from_utf8_lossy(&naptr.regexp).into_owned(),
                replacement: naptr.replacement.to_utf8().trim_end_matches('.').to_owned(),
            }),
            _ => None,
        })
        .collect();
    Ok(entry::with_runtime(|context| {
        let mut items = Vec::with_capacity(records.len());
        for record in &records {
            let object = entry::make_object(context);
            let flags_v = entry::make_string(context, &record.flags);
            entry::put_member(context, object, "flags", flags_v);
            let service_v = entry::make_string(context, &record.services);
            entry::put_member(context, object, "service", service_v);
            let regexp_v = entry::make_string(context, &record.regexp);
            entry::put_member(context, object, "regexp", regexp_v);
            let replacement_v = entry::make_string(context, &record.replacement);
            entry::put_member(context, object, "replacement", replacement_v);
            let order_v = entry::make_number(f64::from(record.order));
            entry::put_member(context, object, "order", order_v);
            let preference_v = entry::make_number(f64::from(record.preference));
            entry::put_member(context, object, "preference", preference_v);
            items.push(object);
        }
        entry::make_array_in(context, items)
    }))
}
