//! `dns.resolveMx`/`resolveSrv` — the two record types whose whole answer is
//! an array of small structured objects over a name (`{priority, exchange}`,
//! `{priority, weight, port, name}`), each with a dedicated
//! `hickory-resolver` method (`mx_lookup`/`srv_lookup`).

use super::client::{node_code, resolver, runtime};
use super::common::callback_result;
use hickory_resolver::TokioResolver;
use hickory_resolver::proto::rr::RData;
use rts_core::entry;

/// `dns.resolveMx(hostname, callback)` → `Array<{priority, exchange}>`.
pub(super) extern "C" fn resolve_mx(_e: u64, _this: u64, hostname: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    callback_result(hostname, callback, "queryMx", |host| {
        let resolver = resolver().map_err(|err| node_code(&err))?;
        mx_value(resolver, host)
    })
}

pub(super) fn mx_value(resolver: &TokioResolver, host: &str) -> Result<u64, &'static str> {
    let lookup = runtime().block_on(resolver.mx_lookup(host)).map_err(|err| node_code(&err))?;
    let records: Vec<(u16, String)> = lookup
        .answers()
        .iter()
        .filter_map(|record| match &record.data {
            RData::MX(mx) => Some((mx.preference, mx.exchange.to_utf8().trim_end_matches('.').to_owned())),
            _ => None,
        })
        .collect();
    Ok(entry::with_runtime(|context| {
        let mut items = Vec::with_capacity(records.len());
        for (preference, exchange) in &records {
            let object = entry::make_object(context);
            let priority_v = entry::make_number(f64::from(*preference));
            entry::put_member(context, object, "priority", priority_v);
            let exchange_v = entry::make_string(context, exchange);
            entry::put_member(context, object, "exchange", exchange_v);
            items.push(object);
        }
        entry::make_array_in(context, items)
    }))
}

/// `dns.resolveSrv(hostname, callback)` → `Array<{priority, weight, port, name}>`.
pub(super) extern "C" fn resolve_srv(_e: u64, _this: u64, hostname: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    callback_result(hostname, callback, "querySrv", |host| {
        let resolver = resolver().map_err(|err| node_code(&err))?;
        srv_value(resolver, host)
    })
}

pub(super) fn srv_value(resolver: &TokioResolver, host: &str) -> Result<u64, &'static str> {
    let lookup = runtime().block_on(resolver.srv_lookup(host)).map_err(|err| node_code(&err))?;
    let records: Vec<(u16, u16, u16, String)> = lookup
        .answers()
        .iter()
        .filter_map(|record| match &record.data {
            RData::SRV(srv) => Some((srv.priority, srv.weight, srv.port, srv.target.to_utf8().trim_end_matches('.').to_owned())),
            _ => None,
        })
        .collect();
    Ok(entry::with_runtime(|context| {
        let mut items = Vec::with_capacity(records.len());
        for (priority, weight, port, name) in &records {
            let object = entry::make_object(context);
            let priority_v = entry::make_number(f64::from(*priority));
            entry::put_member(context, object, "priority", priority_v);
            let weight_v = entry::make_number(f64::from(*weight));
            entry::put_member(context, object, "weight", weight_v);
            let port_v = entry::make_number(f64::from(*port));
            entry::put_member(context, object, "port", port_v);
            let name_v = entry::make_string(context, name);
            entry::put_member(context, object, "name", name_v);
            items.push(object);
        }
        entry::make_array_in(context, items)
    }))
}
