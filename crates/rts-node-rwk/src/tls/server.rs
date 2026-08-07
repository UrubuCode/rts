//! `tls.createServer`/`tls.Server` — wraps a `net.Server`'s raw accepted
//! sockets the same way `socket.rs` wraps a client's, over the same
//! [`super::conn::Driver`]/[`super::registry`]; see `socket.rs`'s own "WHEN"
//! section, which applies here unchanged (the accepted socket's `'data'` is
//! still what drives everything, now server-side).
//!
//! # Not implemented, by name
//!
//! `SNICallback`/`addContext` (one `SecureContext` serves every
//! connection), `ALPNCallback`, client-certificate request/verification,
//! `getTicketKeys`/`setTicketKeys`, `'keylog'`/`'OCSPRequest'`/
//! `'newSession'`/`'resumeSession'`/`'tlsClientError'`.

use rts_core_rwk::entry::{self, Provided};

use super::conn::{Driver, Side};
use super::registry;

const METHODS: &[(&str, Provided)] = &[("close", close)];

fn prototype(context: &mut entry::Context) -> u64 {
    super::common::chained_prototype(context, "Server", "TLSServer", METHODS)
}

/// `tls.createServer(options?[, secureConnectionListener])`.
pub(super) extern "C" fn create_server(_e: u64, _this: u64, options: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let context_id = entry::with_runtime(|context| super::common::option_value(context, options, "secureContext"))
        .and_then(super::context::context_id)
        .or_else(|| {
            // No pre-built `SecureContext`: build one from `cert`/`key`/`ca`
            // fields on `options` itself, the common `createServer({cert,
            // key})` shape.
            let instance = super::context::build(options);
            super::context::context_id(instance)
        });

    let server_instance = entry::with_runtime(|context| {
        let proto = prototype(context);
        entry::make_instance(context, proto)
    });
    entry::with_runtime(|context| super::common::init_emitter(context, server_instance));
    if listener != absent {
        let once_fn = entry::with_runtime(|context| entry::get_member(context, server_instance, "once"));
        if once_fn != absent {
            let event = super::common::key("secureConnection");
            entry::call(once_fn, server_instance, event, listener, absent, absent);
        }
    }

    let underlying = entry::with_runtime(|context| {
        let net_ns = crate::net::namespace(context);
        let ctor = entry::get_member(context, net_ns, "createServer");
        entry::call(ctor, absent, absent, absent, absent, absent)
    });
    entry::with_runtime(|context| super::common::set_value(context, server_instance, "__underlyingServer", underlying));
    entry::with_runtime(|context| super::common::set_num(context, underlying, "__tlsContextId", context_id.unwrap_or(0) as f64));
    entry::with_runtime(|context| super::common::set_value(context, underlying, "__tlsServerInstance", server_instance));

    let on_connection = entry::with_runtime(|context| entry::make_callable(context, on_connection));
    let on_fn = entry::with_runtime(|context| entry::get_member(context, underlying, "on"));
    if on_fn != absent {
        let event = super::common::key("connection");
        entry::call(on_fn, underlying, event, on_connection, absent, absent);
    }

    server_instance
}

/// The underlying `net.Server`'s `'connection'` listener — a raw,
/// not-yet-handshaked socket, per `docs/reference/node/tls.md` §2's own note
/// that this ordering differs from `https.Server`'s.
extern "C" fn on_connection(_e: u64, this: u64, raw_socket: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let (server_instance, context_id) = entry::with_runtime(|context| {
        let server_instance = entry::get_member(context, this, "__tlsServerInstance");
        let context_id = entry::number_of(super::common::get_value(this, "__tlsContextId")).map(|v| v as u64);
        (server_instance, context_id)
    });

    let Some(id) = context_id.filter(|&id| id != 0) else {
        // No cert/key: every handshake will fail once bytes arrive; refused
        // at construction is nicer, but `createServer({})` is a real,
        // named-elsewhere Node shape (`docs/reference/node/tls.md` §2's own
        // note that a context-less server can be built), so this lets the
        // handshake itself surface the error instead.
        return absent;
    };
    let Some((chain, key)) = super::context::identity_for(id) else { return absent };

    let provider = super::provider::provider();
    let Ok(builder) = rustls::ServerConfig::builder_with_provider(provider).with_protocol_versions(&[&rustls::version::TLS13]) else {
        return absent;
    };
    let Ok(config) = builder.with_no_client_auth().with_single_cert(chain, key) else {
        return absent;
    };
    let Ok(conn) = rustls::ServerConnection::new(std::sync::Arc::new(config)) else {
        return absent;
    };

    let tls_id = registry::next_id();
    let tls_instance = entry::with_runtime(|context| {
        let proto = super::socket::prototype(context);
        let instance = entry::make_instance(context, proto);
        super::common::init_emitter(context, instance);
        super::common::set_bool(context, instance, "encrypted", true);
        super::common::set_num(context, instance, "__tlsId", tls_id as f64);
        let write_fn = entry::make_callable(context, super::socket::write_hook);
        entry::put_member(context, instance, "_write", write_fn);
        instance
    });
    registry::with_entries(|table| {
        table.insert(
            tls_id,
            registry::Entry { driver: Driver { side: Side::Server(conn) }, underlying: raw_socket, tls_instance, handshake_done: false },
        );
    });
    entry::with_runtime(|context| super::common::set_num(context, raw_socket, "__tlsPeerId", tls_id as f64));

    let data_hook = entry::with_runtime(|context| entry::make_callable(context, super::socket::on_data));
    let on_fn = entry::with_runtime(|context| entry::get_member(context, raw_socket, "on"));
    if on_fn != absent {
        let event = super::common::key("data");
        entry::call(on_fn, raw_socket, event, data_hook, absent, absent);
    }

    // `'secureConnection'` fires once the handshake completes; that happens
    // inside the shared `on_data` handler, which (server-side too) emits
    // `'secureConnect'`/`'secure'` on the `TLSSocket` — this server also
    // wants `'secureConnection'` with the socket, so register a one-shot
    // relay.
    let relay = entry::with_runtime(|context| entry::make_callable(context, relay_secure_connection));
    entry::with_runtime(|context| super::common::set_value(context, tls_instance, "__tlsServer", server_instance));
    let once_fn = entry::with_runtime(|context| entry::get_member(context, tls_instance, "once"));
    if once_fn != absent {
        let event = super::common::key("secureConnect");
        entry::call(once_fn, tls_instance, event, relay, absent, absent);
    }
    absent
}

extern "C" fn relay_secure_connection(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let server = entry::with_runtime(|context| entry::get_member(context, this, "__tlsServer"));
    if server != absent {
        super::common::emit(server, "secureConnection", this, absent, absent);
    }
    absent
}

extern "C" fn close(_e: u64, this: u64, callback: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let underlying = entry::with_runtime(|context| entry::get_member(context, this, "__underlyingServer"));
    if underlying != absent {
        let close_fn = entry::with_runtime(|context| entry::get_member(context, underlying, "close"));
        if close_fn != absent {
            entry::call(close_fn, underlying, callback, absent, absent, absent);
        }
    }
    this
}
