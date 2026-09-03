//! `dns.resolveAny` — one query, tagged by `type` per answer. See the module
//! doc's "Not implemented, by name" and `docs/reference/node/dns.md` §4 for
//! why this is inherently best-effort (`ANY` is a meta-QTYPE, not a real RR
//! type, and Node's own docs disclaim completeness): whatever the configured
//! server chooses to answer for an `ANY` query — some omit types even when
//! present, and RFC 8482 recommends refusing it outright — is what this
//! reports, unfiltered.
//!
//! Node's `AnyRecord` union (`docs/reference/node/dns.md` §3) covers eight
//! shapes; this decodes exactly those eight (A, AAAA, CNAME, MX, NAPTR, NS,
//! PTR, SOA, SRV, TXT — the union lists ten, A/AAAA/TXT counted once each).
//! CAA and TLSA are not part of that union — real Node's own `resolveAny`
//! does not surface them either — so an answer of either kind is skipped
//! here the same way any OTHER record type would be, rather than invented a
//! tenth shape the reference does not document.

use super::client::{node_code, resolver, runtime};
use super::common::callback_result;
use hickory_resolver::TokioResolver;
use hickory_resolver::proto::rr::{RData, Record, RecordType};
use rts_core::entry;

/// `dns.resolveAny(hostname, callback)`.
pub(super) extern "C" fn resolve_any(_e: u64, _this: u64, hostname: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    callback_result(hostname, callback, "resolveAny", |host| {
        let resolver = resolver().map_err(|err| node_code(&err))?;
        value(resolver, host)
    })
}

pub(super) fn value(resolver: &TokioResolver, host: &str) -> Result<u64, &'static str> {
    let lookup = runtime().block_on(resolver.lookup(host, RecordType::ANY)).map_err(|err| node_code(&err))?;
    Ok(entry::with_runtime(|context| {
        let items: Vec<u64> = lookup.answers().iter().filter_map(|record| tagged(context, record)).collect();
        entry::make_array_in(context, items)
    }))
}

fn name_text(name: &hickory_resolver::proto::rr::Name) -> String {
    name.to_utf8().trim_end_matches('.').to_owned()
}

/// One `AnyRecord` element, or `None` for a record type outside the eight
/// this file's module doc names (an `OPT`/`HINFO`/other answer the server
/// included, or a CAA/TLSA one — see that doc for why those two specifically
/// are excluded on purpose rather than by omission).
fn tagged(context: &mut entry::Context, record: &Record) -> Option<u64> {
    let object = entry::make_object(context);
    let type_name: &str;
    match &record.data {
        RData::A(address) => {
            type_name = "A";
            let address_v = entry::make_string(context, &address.0.to_string());
            entry::put_member(context, object, "address", address_v);
            let ttl_v = entry::make_number(f64::from(record.ttl));
            entry::put_member(context, object, "ttl", ttl_v);
        }
        RData::AAAA(address) => {
            type_name = "AAAA";
            let address_v = entry::make_string(context, &address.0.to_string());
            entry::put_member(context, object, "address", address_v);
            let ttl_v = entry::make_number(f64::from(record.ttl));
            entry::put_member(context, object, "ttl", ttl_v);
        }
        RData::CNAME(name) => {
            type_name = "CNAME";
            let value_v = entry::make_string(context, &name_text(&name.0));
            entry::put_member(context, object, "value", value_v);
        }
        RData::NS(name) => {
            type_name = "NS";
            let value_v = entry::make_string(context, &name_text(&name.0));
            entry::put_member(context, object, "value", value_v);
        }
        RData::PTR(name) => {
            type_name = "PTR";
            let value_v = entry::make_string(context, &name_text(&name.0));
            entry::put_member(context, object, "value", value_v);
        }
        RData::MX(mx) => {
            type_name = "MX";
            let priority_v = entry::make_number(f64::from(mx.preference));
            entry::put_member(context, object, "priority", priority_v);
            let exchange_v = entry::make_string(context, &name_text(&mx.exchange));
            entry::put_member(context, object, "exchange", exchange_v);
        }
        RData::SRV(srv) => {
            type_name = "SRV";
            let priority_v = entry::make_number(f64::from(srv.priority));
            entry::put_member(context, object, "priority", priority_v);
            let weight_v = entry::make_number(f64::from(srv.weight));
            entry::put_member(context, object, "weight", weight_v);
            let port_v = entry::make_number(f64::from(srv.port));
            entry::put_member(context, object, "port", port_v);
            let name_v = entry::make_string(context, &name_text(&srv.target));
            entry::put_member(context, object, "name", name_v);
        }
        RData::SOA(soa) => {
            type_name = "SOA";
            let nsname_v = entry::make_string(context, &name_text(&soa.mname));
            entry::put_member(context, object, "nsname", nsname_v);
            let hostmaster_v = entry::make_string(context, &name_text(&soa.rname));
            entry::put_member(context, object, "hostmaster", hostmaster_v);
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
        }
        RData::NAPTR(naptr) => {
            type_name = "NAPTR";
            let flags_v = entry::make_string(context, &String::from_utf8_lossy(&naptr.flags));
            entry::put_member(context, object, "flags", flags_v);
            let service_v = entry::make_string(context, &String::from_utf8_lossy(&naptr.services));
            entry::put_member(context, object, "service", service_v);
            let regexp_v = entry::make_string(context, &String::from_utf8_lossy(&naptr.regexp));
            entry::put_member(context, object, "regexp", regexp_v);
            let replacement_v = entry::make_string(context, &name_text(&naptr.replacement));
            entry::put_member(context, object, "replacement", replacement_v);
            let order_v = entry::make_number(f64::from(naptr.order));
            entry::put_member(context, object, "order", order_v);
            let preference_v = entry::make_number(f64::from(naptr.preference));
            entry::put_member(context, object, "preference", preference_v);
        }
        RData::TXT(txt) => {
            type_name = "TXT";
            let entries: Vec<u64> = txt.txt_data.iter().map(|chunk| entry::make_string(context, &String::from_utf8_lossy(chunk))).collect();
            let entries_v = entry::make_array_in(context, entries);
            entry::put_member(context, object, "entries", entries_v);
        }
        _ => return None,
    }
    let type_v = entry::make_string(context, type_name);
    entry::put_member(context, object, "type", type_v);
    Some(object)
}
