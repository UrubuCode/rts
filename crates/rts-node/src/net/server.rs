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

use rts_core::entry::{self, Provided};
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
    let (options, listener) = if is_callable(a) { (absent, a) } else { (a, b) };
    if options != absent {
        // `path` — the Unix-socket/named-pipe IPC form. Never implemented
        // (only the TCP `(port, host?, callback?)` overload `listen` reads),
        // so Node's own option is now refused by name rather than silently
        // accepted and then ignored the first time `.listen()` is called.
        let path = entry::with_runtime(|context| entry::get_member(context, options, "path"));
        if path != absent {
            entry::throw_type_error("ERR_INVALID_ARG_VALUE: The 'options.path' argument is not implemented");
            return absent;
        }
        // `blockList` — validated the same way `net.BlockList.isBlockList`
        // does; see `dgram/mod.rs`'s identical check for
        // `sendBlockList`/`receiveBlockList` (this crate's own duplicate of
        // that duck-type test, `net::blocklist::is_block_list` being
        // `pub(super)` to a module this file is already inside — reached
        // directly here instead).
        let block_list = entry::with_runtime(|context| entry::get_member(context, options, "blockList"));
        if block_list != absent && entry::get_indexed(block_list, super::common::key("rules")) == absent {
            entry::throw_type_error(
                "ERR_INVALID_ARG_TYPE: The \"options.blockList\" property must be an instance of net.BlockList. Received an instance that is not one",
            );
            return absent;
        }
    }
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
        // Already listening (or listening once) — real Node's
        // `ERR_SERVER_ALREADY_LISTEN`, an `Error` (not a `TypeError`);
        // `entry::throw_type_error` is the only raise this crate can reach
        // publicly (rule 8's exemption list does not cover a second error
        // class), so the class diverges while the code/message text — what
        // this file's own test checks — does not. This used to refuse
        // silently, matching this crate's no-throw convention for gaps it
        // cannot report; a second `listen()` is not a gap, it is a real
        // Node error this crate CAN report, just under the wrong class.
        entry::throw_type_error("ERR_SERVER_ALREADY_LISTEN: Listen method has been called more than once without closing.");
        return this;
    }
    let Some((port, host, callback)) = bind_target(a, b, c) else {
        // A refusal is a throw already REGISTERED plus a return — see
        // `validate`'s own doc.
        return this;
    };
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

/// Where `listen` was asked to bind, with every argument checked.
///
/// `None` means a refusal is already registered and the caller must return —
/// see [`super::validate`]'s doc for the shape and for why nothing here raises
/// from inside a runtime borrow.
///
/// Three overloads reach here: `listen([callback])` (a random port, which is
/// what `listen()` means and what half of Node's own suite opens with),
/// `listen(port[, host][, callback])`, and `listen(options[, callback])`. The
/// handle form is still absent, per the module doc.
fn bind_target(a: u64, b: u64, c: u64) -> Option<(u16, String, u64)> {
    let absent = entry::undefined_value();
    let any_port = "0.0.0.0".to_owned();
    if a == absent {
        return Some((0, any_port, absent));
    }
    // `listen(callback)` — the port is left to the OS. It used to fall into the
    // port branch below, which read the function as port `0` correctly and then
    // lost the callback entirely, because the callback slot was only ever read
    // from `b`.
    if is_callable(a) {
        return Some((0, any_port, a));
    }
    let options = entry::with_runtime(|context| {
        if entry::string_in(context, a).is_some() || !entry::is_object(context, a) {
            return None;
        }
        // One statement per read: `get_member` takes the context uniquely and
        // `string_in` takes it shared, so nesting them is a borrow conflict.
        let port = entry::get_member(context, a, "port");
        let path = entry::get_member(context, a, "path");
        let named = entry::get_member(context, a, "host");
        let host = entry::string_in(context, named);
        Some((port, path, host))
    });
    if let Some((port, path, host)) = options {
        if path != absent {
            // IPC is not implemented (module doc); refused by name rather than
            // ignored, which would bind a TCP port the program never asked for.
            crate::errors::invalid_arg_value("options.path", path, "is not supported");
            return None;
        }
        if port == absent {
            crate::errors::invalid_arg_value("options", a, "must have the property \"port\" or \"path\"");
            return None;
        }
        let port = super::validate::port("options.port", port)?;
        return Some((port, host.unwrap_or(any_port), b));
    }
    // A non-numeric STRING is a path, not a bad port. `listen('/tmp/x.sock')`
    // is Node's IPC form, and running it through the port validator reported
    // `port should be >= 0 and < 65536. Received /pipe/node-test.sock` — an
    // error that names the wrong argument and sends a reader looking for a
    // number they never wrote. The port validator accepts a numeric string
    // (`'8080'`), so what falls through here is exactly the path form.
    //
    // Refused rather than bound to a TCP port nobody asked for: IPC is not
    // implemented (see this module's doc), and an absent surface fails at the
    // call.
    let text = entry::with_runtime(|context| entry::string_in(context, a));
    let is_path = text.is_some_and(|held| held.trim().parse::<f64>().is_err());
    if is_path {
        crate::errors::invalid_arg_value("path", a, "is not supported");
        return None;
    }
    let port = super::validate::port("port", a)?;
    // Asked with the type TEST and not with `ToString`. `text_of` converts, so
    // it answers `Some("undefined")` for an argument that was never passed —
    // which made `listen(port)` bind to a host literally named "undefined", fail
    // to resolve, and emit an `'error'` nothing handled. It also made the
    // overload test always take the `(port, host, callback)` branch, so
    // `listen(port, callback)` lost its callback.
    let named = entry::with_runtime(|context| entry::string_in(context, b));
    let callback = if named.is_some() { c } else { b };
    Some((port, named.unwrap_or(any_port), callback))
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

/// `server.address()` — `null` before `'listening'`/after `close()`, else
/// `{ port, family, address }`. `port`/`family` used to be missing entirely —
/// only the combined `"host:port"` string landed under `address`, so
/// `server.address().port` read `undefined` for every caller, including this
/// module's own doc comments elsewhere describing the shape it should be.
extern "C" fn address(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    registry::pump();
    let Some(id) = server_id(this) else { return entry::null_value() };
    // `null` after `close()` — this used to keep answering the last-known
    // address forever, because `close()` flips `ServerEntry::closed` (which
    // stops delivering EVENTS) but this read never checked it, so a closed
    // server's `address()` looked identical to a listening one.
    let local = registry::with_servers(|table| table.get(&id).filter(|e| !e.closed).and_then(|e| e.local_addr.clone()));
    let Some(local) = local else { return entry::null_value() };
    let parsed: Option<std::net::SocketAddr> = local.parse().ok();
    let (host, port, family) = match parsed {
        Some(std::net::SocketAddr::V4(v4)) => (v4.ip().to_string(), v4.port(), "IPv4"),
        Some(std::net::SocketAddr::V6(v6)) => (v6.ip().to_string(), v6.port(), "IPv6"),
        None => (local.clone(), 0, "IPv4"),
    };
    entry::with_runtime(|context| {
        let object = entry::make_object(context);
        let address_v = entry::make_string(context, &host);
        let port_v = entry::make_number(port as f64);
        let family_v = entry::make_string(context, family);
        entry::put_member(context, object, "address", address_v);
        entry::put_member(context, object, "port", port_v);
        entry::put_member(context, object, "family", family_v);
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
