//! `tls.connect(...)` and `TLSSocket` — a `Duplex` chained onto `node:net`'s
//! `"Socket"` prototype (`docs/reference/node/tls.md` §2's own shape), by
//! WRAPPING a real `net.Socket` rather than opening a `TcpStream` itself:
//! `net::socket`/`net::registry` are private to that module, so the only
//! reach this module has into a live connection is `node:net`'s own JS
//! surface (`net.connect`), gotten by name off [`crate::net::namespace`].
//!
//! # WHEN `'secureConnect'`/`'data'` fires — read before relying on it
//!
//! Ciphertext for the underlying `net.Socket` arrives on ITS OWN reader
//! thread (`net/socket.rs`'s own doc), and is delivered to THIS module only
//! as a `'data'` event on that socket — which, per `net`'s own "WHEN"
//! section, fires synchronously at the start of the next `node:net` call.
//! [`on_data`] (registered as that listener) is what feeds ciphertext to
//! rustls and calls `'secureConnect'`/`'data'` on the `TLSSocket`, so a
//! `TLSSocket`'s own listeners fire at exactly the same point the wrapped
//! `net.Socket`'s do: the start of the next `node:net` call the program
//! makes (`socket.write()`, most commonly) — never on a timer, never handed
//! control back on its own.
//!
//! # Not implemented, by name (this file's share of `mod.rs`'s list)
//!
//! SNI (`servername` is read for the handshake's `ServerName` but no
//! `SNICallback`/per-hostname context exists), ALPN, PSK, session
//! resumption/tickets, renegotiation, `enableTrace`, `exportKeyingMaterial`,
//! `getPeerCertificate`'s field detail (answers `{}` — no X.509 field
//! reader; see `provider/verify.rs`'s own doc for why one isn't pulled in),
//! the `path`/Unix-socket overload of `connect`.

use rts_core::entry::{self, Provided};
use rustls::pki_types::ServerName;

use super::conn::{Driver, Side};
use super::registry;

const METHODS: &[(&str, Provided)] = &[
    ("getProtocol", get_protocol),
    ("isSessionReused", is_session_reused),
    ("getCipher", get_cipher),
    ("getPeerCertificate", get_peer_certificate),
];

pub(super) fn prototype(context: &mut entry::Context) -> u64 {
    super::common::chained_prototype(context, "Socket", "TLSSocket", METHODS)
}

/// `tls.connect(port[, host][, options][, callback])` /
/// `tls.connect(options[, callback])`. The `path`/existing-`socket` and
/// per-argument-position overloads beyond these two are not read.
pub(super) extern "C" fn connect(_e: u64, _this: u64, a: u64, b: u64, c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let (host, port, servername, context_id, listener) = entry::with_runtime(|context| {
        if let Some(port) = entry::number_of(a) {
            let host = entry::text_in(context, b).unwrap_or_else(|| "localhost".to_owned());
            (host.clone(), port as u16, host, None, c)
        } else {
            let host = super::common::option_text(context, a, "host").unwrap_or_else(|| "localhost".to_owned());
            let port = super::common::option_num(context, a, "port").unwrap_or(0.0) as u16;
            let servername = super::common::option_text(context, a, "servername").unwrap_or_else(|| host.clone());
            let secure_context = super::common::option_value(context, a, "secureContext");
            let context_id = secure_context.and_then(super::context::context_id);
            (host, port, servername, context_id, b)
        }
    });

    let provider = super::provider::provider();
    let roots = super::context::roots_for(context_id);
    let Ok(builder) = rustls::ClientConfig::builder_with_provider(provider).with_protocol_versions(&[&rustls::version::TLS13]) else {
        return absent;
    };
    let config = builder.with_root_certificates(roots).with_no_client_auth();
    let Ok(name) = ServerName::try_from(servername).map(|n| n.to_owned()) else {
        return absent;
    };
    let Ok(conn) = rustls::ClientConnection::new(std::sync::Arc::new(config), name) else {
        return absent;
    };

    // The underlying `net.Socket` — built through `node:net`'s own public
    // surface, per this file's own doc, never a `TcpStream` opened here.
    // Read under the borrow, called after it. Calling `net.connect` inside was
    // an abort — it is a native that takes the borrow itself — and it is the
    // same fault `tls/server.rs` had one function away.
    let (connect_fn, port_v, host_v) = entry::with_runtime(|context| {
        let net_ns = crate::net::namespace(context);
        let connect_fn = entry::get_member(context, net_ns, "connect");
        let port_v = entry::make_number(port as f64);
        let host_v = entry::make_string(context, &host);
        (connect_fn, port_v, host_v)
    });
    let underlying = entry::call(connect_fn, absent, port_v, host_v, absent, absent);

    let tls_instance = entry::with_runtime(|context| {
        let proto = prototype(context);
        let instance = entry::make_instance(context, proto);
        super::common::init_emitter(context, instance);
        super::common::set_bool(context, instance, "encrypted", true);
        super::common::set_value(context, instance, "readable", entry::boolean_value(true));
        super::common::set_value(context, instance, "writable", entry::boolean_value(true));
        let write_fn = entry::make_callable(context, write_hook);
        entry::put_member(context, instance, "_write", write_fn);
        let destroy_fn = entry::make_callable(context, destroy_hook);
        entry::put_member(context, instance, "_destroy", destroy_fn);
        instance
    });

    let id = registry::next_id();
    entry::with_runtime(|context| super::common::set_num(context, tls_instance, "__tlsId", id as f64));
    registry::with_entries(|table| {
        table.insert(
            id,
            registry::Entry { driver: Driver { side: Side::Client(conn) }, underlying, tls_instance, handshake_done: false },
        );
    });

    if listener != absent {
        let once_fn = entry::with_runtime(|context| entry::get_member(context, tls_instance, "once"));
        if once_fn != absent {
            let event = super::common::key("secureConnect");
            entry::call(once_fn, tls_instance, event, listener, absent, absent);
        }
    }

    // Wire the underlying socket's own events to drive the handshake and
    // relay data — see this file's "WHEN" section for exactly when these
    // run.
    let data_hook = entry::with_runtime(|context| entry::make_callable(context, on_data));
    let connect_hook = entry::with_runtime(|context| entry::make_callable(context, on_underlying_connect));
    let close_hook = entry::with_runtime(|context| entry::make_callable(context, on_underlying_close));
    entry::with_runtime(|context| super::common::set_num(context, underlying, "__tlsPeerId", id as f64));
    for (event, hook) in [("data", data_hook), ("connect", connect_hook), ("close", close_hook)] {
        let on_fn = entry::with_runtime(|context| entry::get_member(context, underlying, "on"));
        if on_fn != absent {
            let event_key = super::common::key(event);
            entry::call(on_fn, underlying, event_key, hook, absent, absent);
        }
    }

    tls_instance
}

fn peer_id(underlying: u64) -> Option<u64> {
    let value = super::common::get_value(underlying, "__tlsPeerId");
    entry::number_of(value).map(|v| v as u64)
}

fn socket_id(this: u64) -> Option<u64> {
    let value = super::common::get_value(this, "__tlsId");
    entry::number_of(value).map(|v| v as u64)
}

/// Writes whatever ciphertext a [`Driver`] currently has queued to the
/// underlying `net.Socket`'s own `write()` — the JS-level composition this
/// file's doc names, never a raw `TcpStream` write.
fn flush_outgoing(id: u64) {
    let (underlying, bytes) = registry::with_entries(|table| {
        let Some(entry) = table.get_mut(&id) else { return (0, Vec::new()) };
        (entry.underlying, entry.driver.outgoing())
    });
    if bytes.is_empty() || underlying == 0 {
        return;
    }
    let absent = entry::undefined_value();
    let write_fn = entry::with_runtime(|context| entry::get_member(context, underlying, "write"));
    if write_fn != absent {
        let chunk = entry::with_runtime(|context| entry::make_bytes(context, &bytes));
        entry::call(write_fn, underlying, chunk, absent, absent, absent);
    }
}

extern "C" fn on_underlying_connect(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if let Some(id) = peer_id(this) {
        flush_outgoing(id);
    }
    entry::undefined_value()
}

extern "C" fn on_underlying_close(_e: u64, this: u64, had_error: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if let Some(id) = peer_id(this) {
        let tls_instance = registry::with_entries(|table| table.get(&id).map(|e| e.tls_instance));
        if let Some(tls_instance) = tls_instance {
            super::common::emit(tls_instance, "close", had_error, entry::undefined_value(), entry::undefined_value());
        }
    }
    entry::undefined_value()
}

/// The underlying `net.Socket`'s `'data'` listener — feeds ciphertext to
/// rustls, relays plaintext and the once-only `'secureConnect'`/`'secure'`.
pub(super) extern "C" fn on_data(_e: u64, this: u64, chunk: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let Some(id) = peer_id(this) else { return absent };
    let bytes = entry::with_runtime(|context| entry::bytes_of(context, chunk)).unwrap_or_default();

    let (fed_plaintext, just_connected, closed, tls_instance) = registry::with_entries(|table| {
        let Some(entry) = table.get_mut(&id) else {
            return (Vec::new(), false, false, 0);
        };
        let fed = entry.driver.feed(&bytes);
        if fed.just_connected {
            entry.handshake_done = true;
        }
        (fed.plaintext, fed.just_connected, fed.closed, entry.tls_instance)
    });
    if tls_instance == 0 {
        return absent;
    }
    flush_outgoing(id);

    if just_connected {
        super::common::emit(tls_instance, "secureConnect", absent, absent, absent);
        super::common::emit(tls_instance, "secure", absent, absent, absent);
    }
    if !fed_plaintext.is_empty() {
        let push_fn = entry::with_runtime(|context| entry::get_member(context, tls_instance, "push"));
        if push_fn != absent {
            let data = entry::with_runtime(|context| entry::make_bytes(context, &fed_plaintext));
            entry::call(push_fn, tls_instance, data, absent, absent, absent);
        }
    }
    if closed {
        super::common::emit(tls_instance, "error", absent, absent, absent);
    }
    absent
}

/// Installed as `this._write` — encrypts and forwards to the underlying
/// socket; see this file's "WHEN" doc for when the peer actually observes
/// it (immediately — `flush_outgoing` writes synchronously here).
pub(super) extern "C" fn write_hook(_e: u64, this: u64, chunk: u64, _encoding: u64, callback: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let Some(id) = socket_id(this) else {
        entry::call(callback, absent, absent, absent, absent, absent);
        return absent;
    };
    let bytes = entry::text_of(chunk)
        .and_then(|text| entry::encode_text(&text, "utf8"))
        .or_else(|| entry::with_runtime(|context| entry::bytes_of(context, chunk)))
        .unwrap_or_default();
    registry::with_entries(|table| {
        if let Some(entry) = table.get_mut(&id) {
            entry.driver.send(&bytes);
        }
    });
    flush_outgoing(id);
    entry::call(callback, absent, absent, absent, absent, absent);
    absent
}

extern "C" fn destroy_hook(_e: u64, this: u64, _error: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if let Some(id) = socket_id(this) {
        let underlying = registry::with_entries(|table| table.remove(&id).map(|e| e.underlying));
        if let Some(underlying) = underlying {
            let absent = entry::undefined_value();
            let destroy_fn = entry::with_runtime(|context| entry::get_member(context, underlying, "destroy"));
            if destroy_fn != absent {
                entry::call(destroy_fn, underlying, absent, absent, absent, absent);
            }
        }
    }
    entry::undefined_value()
}

extern "C" fn get_protocol(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(id) = socket_id(this) else { return entry::undefined_value() };
    let done = registry::with_entries(|table| table.get(&id).map(|e| e.handshake_done).unwrap_or(false));
    entry::with_runtime(|context| if done { entry::make_string(context, "TLSv1.3") } else { entry::null_in(context) })
}

extern "C" fn is_session_reused(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::boolean_value(false)
}

extern "C" fn get_cipher(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let object = entry::make_object(context);
        let name = entry::make_string(context, "TLS_AES_128_GCM_SHA256");
        let version = entry::make_string(context, "TLSv1.3");
        entry::put_member(context, object, "name", name);
        entry::put_member(context, object, "version", version);
        object
    })
}

/// Always `{}` — see this file's "Not implemented" note on field detail.
extern "C" fn get_peer_certificate(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(entry::make_object)
}
