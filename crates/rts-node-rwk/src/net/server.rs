//! `net.Server` — a real `EventEmitter` (chained the same way
//! `fs/watch.rs`'s `FSWatcher` is), backed by a blocking
//! `std::net::TcpListener` accept loop on its own thread.
//!
//! # `close()` does not stop the accept thread instantly
//!
//! `std::net::TcpListener::accept` has no timeout in `std` alone (no
//! `socket2`/tokio dependency this manifest carries to give it one — see
//! `Cargo.toml`'s vetted-crate comment), so the accept thread can only check
//! [`ServerEntry::stop`] BETWEEN calls to `accept()`, which blocks. A
//! `close()` while the listener is idle (no pending connection) sets the
//! flag and returns; the OS-level socket is not actually released until one
//! more connection arrives (accepted, then immediately dropped without
//! firing `'connection'`) or the process exits. Named, not hidden: a test
//! that binds the same port again right after `close()` can still see
//! `EADDRINUSE` for a brief window — the same class of deviation
//! `fs/watch.rs` already names for its own background thread.

use rts_core_rwk::entry::{self, Provided};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::registry::{self, ServerEntry, ServerEvent};

const METHODS: &[(&str, Provided)] = &[
    ("listen", listen),
    ("close", close),
    ("address", address),
    ("getConnections", get_connections),
    ("ref", noop_self),
    ("unref", noop_self),
];

pub(super) fn prototype(context: &mut entry::Context) -> u64 {
    // Registered as `"net.Server"`, not the bare `"Server"` the JS-visible
    // class is named: `http::server` registers its OWN, differently-shaped
    // `Server` prototype under the bare name too, and `make_prototype` is
    // idempotent BY NAME — whichever module's `namespace()` ran first (install
    // order puts `http` before `net`) won the name, and `net.Server` instances
    // got HTTP's method table (`listen`/`close`/`closeAllConnections`/
    // `closeIdleConnections`/`setTimeout`) instead of their own
    // (`address`/`getConnections`/…), so `server.address()` read "not a
    // function". `tls::server`'s parent name changed with it, below.
    super::common::chained_prototype(context, "EventEmitter", "net.Server", METHODS)
}

/// `new net.Server(options?, connectionListener?)` — also the body of
/// `net.createServer`, which just calls this and skips `new`.
pub(super) extern "C" fn construct(_e: u64, this: u64, a: u64, b: u64, _c: u64, _d: u64) -> u64 {
    registry::pump();
    let absent = entry::undefined_value();
    // Real Node accepts `(connectionListener)` alone too — told apart from
    // `(options, connectionListener)` by whether the first argument is
    // callable, the same test `stream::writable::end` uses for its own
    // positional-overload collapse.
    let (listener,) = if is_callable(a) { (a,) } else { (b,) };
    let instance = entry::with_runtime(|context| {
        let prototype = prototype(context);
        let instance = super::common::self_or_new(context, this, prototype);
        super::common::init_emitter(context, instance);
        super::common::set_bool(context, instance, "listening", false);
        super::common::set_num(context, instance, "maxConnections", 0.0);
        super::common::set_bool(context, instance, "dropMaxConnection", false);
        instance
    });
    if listener != absent {
        let on_fn = entry::with_runtime(|context| entry::get_member(context, instance, "on"));
        if on_fn != absent {
            let event = super::common::key("connection");
            entry::call(on_fn, instance, event, listener, absent, absent);
        }
    }
    instance
}

fn is_callable(value: u64) -> bool {
    let absent = entry::undefined_value();
    value != absent && entry::with_runtime(|context| entry::get_member(context, value, "call")) != absent
}

fn server_id(this: u64) -> Option<u64> {
    let value = entry::get_indexed(this, super::common::key("__serverId"));
    entry::number_of(value).map(|v| v as u64)
}

/// `server.listen([port[, host]][, callback])` — the handle/options/path
/// overloads (`docs/reference/node/net.md` §2) are not read; only the
/// `(port, host?, callback?)` TCP form. See the module doc's own refusal.
extern "C" fn listen(_e: u64, this: u64, a: u64, b: u64, c: u64, _d: u64) -> u64 {
    registry::pump();
    let absent = entry::undefined_value();
    if server_id(this).is_some() {
        // Already listening (or listening once, per this crate's no-throw
        // convention) — `ERR_SERVER_ALREADY_LISTEN` in real Node, refused
        // silently here rather than thrown.
        return this;
    }
    let port = super::common::number_arg(a).unwrap_or(0.0) as u16;
    // Asked with the type TEST and not with `ToString`. `text_of` converts, so
    // it answers `Some("undefined")` for an argument that was never passed —
    // which made `listen(port)` bind to a host literally named "undefined", fail
    // to resolve, and emit an `'error'` nothing handled. It also made the
    // overload test below always take the `(port, host, callback)` branch, so
    // `listen(port, callback)` lost its callback.
    let named = entry::with_runtime(|context| entry::string_in(context, b));
    let host = named.clone().unwrap_or_else(|| "0.0.0.0".to_owned());
    let callback = if named.is_some() { c } else { b };
    if callback != absent {
        let once_fn = entry::with_runtime(|context| entry::get_member(context, this, "once"));
        if once_fn != absent {
            let event = super::common::key("listening");
            entry::call(once_fn, this, event, callback, absent, absent);
        }
    }
    let id = registry::next_id();
    entry::with_runtime(|context| super::common::set_num(context, this, "__serverId", id as f64));
    let stop = Arc::new(AtomicBool::new(false));
    registry::with_servers(|table| {
        table.insert(id, ServerEntry { owner: std::thread::current().id(), instance: this, queue: Default::default(), listening: false, closed: false, stop: stop.clone(), local_addr: None });
    });
    std::thread::spawn(move || match TcpListener::bind((host.as_str(), port)) {
        Ok(listener) => {
            let local = listener.local_addr().map(|a| a.to_string()).unwrap_or_default();
            registry::with_servers(|table| {
                if let Some(entry) = table.get_mut(&id) {
                    entry.queue.push_back(ServerEvent::Listening { local });
                }
            });
            accept_loop(id, listener, stop);
        }
        Err(error) => registry::with_servers(|table| {
            if let Some(entry) = table.get_mut(&id) {
                entry.queue.push_back(ServerEvent::ListenFailed(error.to_string()));
            }
        }),
    });
    this
}

fn accept_loop(id: u64, listener: TcpListener, stop: Arc<AtomicBool>) {
    loop {
        match listener.accept() {
            Ok((stream, addr)) => {
                if stop.load(Ordering::SeqCst) {
                    return;
                }
                registry::with_servers(|table| {
                    if let Some(entry) = table.get_mut(&id)
                        && !entry.closed
                    {
                        entry.queue.push_back(ServerEvent::Accepted(stream, addr.to_string()));
                    }
                });
            }
            Err(error) => {
                if stop.load(Ordering::SeqCst) {
                    return;
                }
                registry::with_servers(|table| {
                    if let Some(entry) = table.get_mut(&id) {
                        entry.queue.push_back(ServerEvent::Error(error.to_string()));
                    }
                });
                return;
            }
        }
    }
}

/// `server.close(callback?)` — see the module doc for why the OS socket may
/// outlive this call by one accepted-and-dropped connection.
extern "C" fn close(_e: u64, this: u64, callback: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    registry::pump();
    let absent = entry::undefined_value();
    let Some(id) = server_id(this) else {
        if callback != absent {
            let error = entry::with_runtime(|context| {
                let object = entry::make_object(context);
                let message = entry::make_string(context, "Server is not running.");
                let code = entry::make_string(context, "ERR_SERVER_NOT_RUNNING");
                entry::put_member(context, object, "message", message);
                entry::put_member(context, object, "code", code);
                object
            });
            entry::call(callback, absent, error, absent, absent, absent);
        }
        return this;
    };
    registry::with_servers(|table| {
        if let Some(entry) = table.get_mut(&id) {
            entry.closed = true;
            entry.stop.store(true, Ordering::SeqCst);
        }
    });
    entry::with_runtime(|context| super::common::set_bool(context, this, "listening", false));
    if callback != absent {
        let once_fn = entry::with_runtime(|context| entry::get_member(context, this, "once"));
        if once_fn != absent {
            let event = super::common::key("close");
            entry::call(once_fn, this, event, callback, absent, absent);
        }
    }
    super::common::emit(this, "close", absent, absent, absent);
    this
}

/// `server.address()` — `null` before `'listening'`/after `close()`.
extern "C" fn address(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    registry::pump();
    let Some(id) = server_id(this) else { return entry::null_value() };
    let local = registry::with_servers(|table| table.get(&id).and_then(|e| e.local_addr.clone()));
    let Some(local) = local else { return entry::null_value() };
    entry::with_runtime(|context| {
        let object = entry::make_object(context);
        let address_v = entry::make_string(context, &local);
        entry::put_member(context, object, "address", address_v);
        object
    })
}

/// `server.getConnections(callback)` — always answers `0`: no per-server
/// connection count is tracked (a `Socket` adopted from `'connection'`
/// carries no back-reference to the `Server` that produced it). Named
/// rather than a number that looks plausible but is not measured.
extern "C" fn get_connections(_e: u64, this: u64, callback: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let null = entry::null_value();
    let zero = entry::make_number(0.0);
    entry::call(callback, absent, null, zero, absent, absent);
    this
}

extern "C" fn noop_self(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    this
}
