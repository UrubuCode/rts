//! `https.createServer`/`https.Server` — an `http.Server` whose connections
//! are `tls.TLSSocket`s instead of raw `net.Socket`s, per the module doc.
//!
//! # How this reuses `http`'s parser/`IncomingMessage`/`ServerResponse`
//! without a second copy of any of them
//!
//! `http::server::construct` already does everything a request needs done —
//! it just does it to whatever `net.Socket` fires `'connection'` on the
//! `net.Server` it builds for itself (`http/server.rs`'s own doc: `on_connection`
//! is registered with `net_server.on('connection', on_connection)` at
//! construct time, an ordinary `EventEmitter` listener registration that
//! does not care what kind of socket it is handed — only that the socket
//! has `'data'`/`'end'`/`resume`, which a `TLSSocket` has too, being chained
//! onto `net`'s own `"Socket"` prototype (`tls/socket.rs`'s own doc)).
//!
//! So this module: (1) builds a real `http.Server` by calling `http`'s own
//! constructor — through `crate::http::namespace`, never `http`'s private
//! `server` module — which does its usual thing and (irrelevantly to this
//! module) also builds a plain `net.Server` it will never listen on; (2)
//! builds a real `tls.Server` the same way, through `crate::tls::namespace`;
//! (3) on the `tls.Server`'s `'secureConnection'`, calls
//! `.emit('connection', tlsSocket)` on the `http.Server`'s own `net.Server`
//! object — the exact event that `on_connection` is already listening for.
//! **No HTTP parsing code exists in this file.** `relay_secure_connection`
//! is the whole mechanism.
//!
//! `listen`/`close` are the two methods this class cannot simply inherit:
//! `tls.Server` has no working `listen` of its own (its own module has no
//! entry for one — only `close`, which forwards to the `net.Server` it
//! holds at `__underlyingServer`), so this shadows both as OWN properties on
//! the composed instance, routing `listen` to that `net.Server` directly and
//! `close` through `tls.Server`'s own (which already forwards correctly).

use rts_core::entry;

use super::common::*;

/// `https.createServer(options[, requestListener])` / `new https.Server(...)`.
pub(super) extern "C" fn construct(_e: u64, _this: u64, options: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();

    let http_server_ctor = entry::with_runtime(|context| http_member(context, "Server"));
    let http_server = entry::call(http_server_ctor, absent, listener, absent, absent, absent);
    let net_server = get_value(http_server, "__netServer__");

    let tls_create_server = entry::with_runtime(|context| tls_member(context, "createServer"));
    let tls_server = entry::call(tls_create_server, absent, options, absent, absent, absent);

    entry::with_runtime(|context| {
        set_value(context, tls_server, "__httpsNetServer", net_server);
        set_value(context, http_server, "__httpsTlsServer", tls_server);
        let listen_fn = entry::make_callable(context, listen);
        entry::put_member(context, http_server, "listen", listen_fn);
        let close_fn = entry::make_callable(context, close);
        entry::put_member(context, http_server, "close", close_fn);
    });

    let on_fn = entry::with_runtime(|context| entry::get_member(context, tls_server, "on"));
    let relay = entry::with_runtime(|context| entry::make_callable(context, relay_secure_connection));
    entry::call(on_fn, tls_server, key("secureConnection"), relay, absent, absent);

    http_server
}

/// `https.createServer(...)` — the plain-function form of [`construct`].
pub(super) extern "C" fn create_server(e: u64, _this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    construct(e, 0, a, b, c, d)
}

/// The `tls.Server`'s `'secureConnection'` listener: hands the now-decrypted
/// `TLSSocket` to `http`'s own `on_connection` by firing the event it is
/// already listening for — see the module doc.
extern "C" fn relay_secure_connection(_e: u64, this: u64, socket: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let net_server = get_value(this, "__httpsNetServer");
    emit(net_server, "connection", socket, entry::undefined_value(), entry::undefined_value());
    entry::undefined_value()
}

/// `server.listen(...)` — forwarded to the `net.Server` the held
/// `tls.Server` itself listens through (`tls.Server` has no working `listen`
/// of its own; see the module doc), never to `http.Server`'s own inert
/// `net.Server`.
extern "C" fn listen(_e: u64, this: u64, a: u64, b: u64, c: u64, _d: u64) -> u64 {
    let tls_server = get_value(this, "__httpsTlsServer");
    let underlying = get_value(tls_server, "__underlyingServer");
    call_method(underlying, "listen", a, b, c);
    entry::with_runtime(|context| set_bool(context, this, "listening", true));
    this
}

/// `server.close(callback?)` — forwarded to the held `tls.Server`, whose own
/// `close` already forwards to its underlying `net.Server`.
extern "C" fn close(_e: u64, this: u64, callback: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let tls_server = get_value(this, "__httpsTlsServer");
    let absent = entry::undefined_value();
    call_method(tls_server, "close", absent, absent, absent);
    entry::with_runtime(|context| set_bool(context, this, "listening", false));
    if callback != absent {
        let once_fn = entry::with_runtime(|context| entry::get_member(context, this, "once"));
        entry::call(once_fn, this, key("close"), callback, absent, absent);
    }
    this
}
