//! `http.Agent` — a plain object (not an `EventEmitter` in real Node either;
//! `docs/reference/node/http.md` §2.1 states that explicitly) carrying the
//! pooling OPTIONS this client honestly does not pool by. See the module's
//! own "not implemented" note: [`super::client`]'s `ClientRequest` opens and
//! connects its own `net.Socket` per request and never consults an `Agent`
//! at all — there is no free-socket list here to be a member of. This class
//! exists so a program that constructs one, reads its properties, or passes
//! it as `options.agent` does not crash; it has nothing to actually pool.

use rts_core::entry::{self, Provided};

use super::common::*;

const METHODS: &[(&str, Provided)] = &[
    ("createConnection", create_connection),
    ("keepSocketAlive", keep_socket_alive),
    ("reuseSocket", reuse_socket),
    ("destroy", destroy),
    ("getName", get_name),
];

pub(super) fn prototype(context: &mut entry::Context) -> u64 {
    chained_prototype(context, "EventEmitter", "Agent", METHODS)
}

/// `new http.Agent(options?)`.
pub(super) extern "C" fn construct(_e: u64, this: u64, options: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = prototype(context);
        let instance = self_or_new(context, this, prototype);
        init_emitter(context, instance);
        let empty = entry::make_object(context);
        set_value(context, instance, "freeSockets", empty);
        let empty = entry::make_object(context);
        set_value(context, instance, "requests", empty);
        let empty = entry::make_object(context);
        set_value(context, instance, "sockets", empty);
        let max_free = option_num(context, options, "maxFreeSockets").unwrap_or(256.0);
        set_num(context, instance, "maxFreeSockets", max_free);
        let max_sockets = option_num(context, options, "maxSockets").unwrap_or(f64::INFINITY);
        set_num(context, instance, "maxSockets", max_sockets);
        let max_total = option_num(context, options, "maxTotalSockets").unwrap_or(f64::INFINITY);
        set_num(context, instance, "maxTotalSockets", max_total);
        instance
    })
}

/// `agent.createConnection(options, callback?)` — opens a `net.Socket` and
/// connects it, exactly what [`super::client::build_request`] already does
/// inline; exposed here for a caller that reaches for it directly.
extern "C" fn create_connection(_e: u64, _this: u64, options: u64, callback: u64, _c: u64, _d: u64) -> u64 {
    let net_ns = entry::with_runtime(super::net_namespace);
    let absent = entry::undefined_value();
    let connect_fn = entry::with_runtime(|context| entry::get_member(context, net_ns, "connect"));
    entry::call(connect_fn, absent, options, callback, absent, absent)
}

extern "C" fn keep_socket_alive(_e: u64, _this: u64, _socket: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::boolean_value(true)
}

extern "C" fn reuse_socket(_e: u64, _this: u64, _socket: u64, _request: u64, _c: u64, _d: u64) -> u64 {
    entry::undefined_value()
}

extern "C" fn destroy(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    this
}

/// `agent.getName(options?)` — `host:port` is the one piece of Node's
/// `host:port:localAddress:family` key this Agent (which pools nothing —
/// see the module doc) has any use for.
extern "C" fn get_name(_e: u64, _this: u64, options: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let host = option_text(context, options, "host").unwrap_or_else(|| "localhost".to_owned());
        let port = option_num(context, options, "port").unwrap_or(80.0);
        entry::make_string(context, &format!("{host}:{port}"))
    })
}
