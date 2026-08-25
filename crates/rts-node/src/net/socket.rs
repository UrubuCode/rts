//! `net.Socket` — a `Duplex` (`docs/reference/node/net.md` §2) backed by a
//! blocking `std::net::TcpStream`, per the module doc's "why blocking, not
//! tokio" note.
//!
//! # How this chains onto `Duplex` without reimplementing it
//!
//! `stream::readable`/`stream::writable`/`stream::duplex` are private
//! submodules of `crate::stream` (`mod readable;`, not `pub mod`) — nothing
//! outside that crate module, this one included, can call their `init`
//! functions. So [`construct`] sets the same named data properties
//! `duplex::duplex_construct` sets (`readableHighWaterMark`,
//! `writableLength`, …; read directly off that file, not guessed) by hand,
//! and [`prototype`] chains a Socket-only method list onto `"Duplex"`,
//! fetched by name — [`rts_core::entry::make_prototype`] hands back the
//! SAME shared object `crate::stream::namespace` already built (see its own
//! doc), never a second one. What is NOT reimplemented is any of
//! `push`/`read`/`write`/`pipe`/backpressure's actual logic: `socket.write()`
//! finds Writable's `write` on that shared prototype, which calls
//! `this._write` — the documented extension hook every `Writable` supports —
//! which [`write_hook`] here installs, exactly the way `options.write` or a
//! subclass's own `_write` would. `'data'` arrives the same way: a reader
//! thread hands bytes to [`registry::pump`](super::registry::pump), which
//! calls the inherited `push()`.
//!
//! # WHEN a `'data'`/`'connection'` listener runs — read before relying on it
//!
//! A socket's bytes arrive on its own reader thread — never the JS thread —
//! so, exactly as `fs/watch.rs` documents for the same reason (this engine's
//! context is thread-local; calling into JS from a foreign thread aborts on
//! the first event), that thread never calls a listener directly. It only
//! ever pushes a native record into [`registry`](super::registry), and
//! delivery happens on the JS thread inside [`super::pump`], which every
//! native this module exports calls first. **What a program observes**: a
//! queued `'data'`/`'connection'`/`'connect'`/`'close'` event's listener
//! runs synchronously, just before the NEXT call this crate's `node:net`
//! surface makes — in practice, at the start of the next `net.*` call the
//! program issues. A program that calls `socket.connect(...)` and then
//! nothing else in `node:net` again never observes `'connect'` fire — there
//! is no timer, no event loop, and no other point this engine hands control
//! back to that this crate can hook. Named, not hidden: the overwhelmingly
//! common shape (`connect`, then `write`/`end`, both `node:net` calls)
//! observes every listener; a program that starts a socket and only awaits a
//! `node:timers` callback, say, does not.

use rts_core::entry::{self, Provided};
use std::io::Read;
use std::net::TcpStream;

use super::registry::{self, SocketEntry, SocketEvent};

const METHODS: &[(&str, Provided)] = &[
    ("connect", connect),
    ("address", address),
    ("setTimeout", set_timeout),
    ("setNoDelay", set_no_delay),
    ("setKeepAlive", set_keep_alive),
    // No event-loop keep-alive accounting exists to hook (the same gap
    // `net::server`'s own `ref`/`unref` name) — accepted and return `this`
    // for chaining, with no OS or scheduling effect.
    ("ref", noop_self),
    ("unref", noop_self),
];

pub(super) fn prototype(context: &mut entry::Context) -> u64 {
    super::common::chained_prototype(context, "Duplex", "Socket", METHODS)
}

/// `new net.Socket(options?)`.
///
/// `options.fd` — adopting an already-open file descriptor — is not
/// implemented (no `TcpStream::from_raw_fd`/socket-duplication wiring here),
/// so it is refused by name, Node's own `ERR_INVALID_ARG_VALUE` shape.
pub(super) extern "C" fn construct(_e: u64, this: u64, options: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    registry::pump();
    let absent = entry::undefined_value();
    if options != absent {
        let fd = entry::with_runtime(|context| entry::get_member(context, options, "fd"));
        if fd != absent {
            entry::throw_type_error("ERR_INVALID_ARG_VALUE: The property 'options.fd' is not implemented");
            return absent;
        }
    }
    entry::with_runtime(|context| {
        let prototype = prototype(context);
        let instance = super::common::self_or_new(context, this, prototype);
        init(context, instance);
        instance
    })
}

/// The Duplex-shaped state every `Socket` starts with — see the module doc
/// for why this is set by hand instead of calling `stream`'s own init.
fn init(context: &mut entry::Context, instance: u64) {
    use super::common::{set_bool, set_num, set_value};
    super::common::init_emitter(context, instance);
    let null = entry::null_in(context);
    set_bool(context, instance, "readableObjectMode", false);
    set_num(context, instance, "readableHighWaterMark", 16384.0);
    set_num(context, instance, "readableLength", 0.0);
    set_value(context, instance, "readableEncoding", null);
    set_bool(context, instance, "readableEnded", false);
    set_value(context, instance, "readableFlowing", null);
    set_bool(context, instance, "readable", true);
    set_bool(context, instance, "readableDidRead", false);
    set_bool(context, instance, "readableAborted", false);
    set_bool(context, instance, "destroyed", false);
    set_bool(context, instance, "closed", false);
    set_value(context, instance, "errored", null);
    super::common::set_array(context, instance, "__buf__", Vec::new());
    super::common::set_array(context, instance, "__pipes__", Vec::new());
    super::common::set_array(context, instance, "__pipeEnd__", Vec::new());
    set_bool(context, instance, "__ended__", false);
    set_bool(context, instance, "__emitClose__", true);
    set_bool(context, instance, "writableObjectMode", false);
    set_num(context, instance, "writableHighWaterMark", 16384.0);
    set_num(context, instance, "writableLength", 0.0);
    set_num(context, instance, "writableCorked", 0.0);
    set_bool(context, instance, "writableEnded", false);
    set_bool(context, instance, "writableFinished", false);
    set_bool(context, instance, "writableNeedDrain", false);
    set_bool(context, instance, "writableAborted", false);
    set_bool(context, instance, "writable", true);
    set_value(context, instance, "__defaultEncoding__", null);
    super::common::set_array(context, instance, "__wqueue__", Vec::new());
    set_bool(context, instance, "allowHalfOpen", false);
    set_bool(context, instance, "connecting", false);
    set_bool(context, instance, "pending", true);
    set_num(context, instance, "bytesRead", 0.0);
    set_num(context, instance, "bytesWritten", 0.0);
    // `readyState` — a plain data property here (no accessor machinery
    // reachable from this crate; see `url/mod.rs`'s own doc for the same
    // wall), so it is the value real Node's `readable && writable` rule
    // gives for the state every fresh `Socket` actually starts in: `"open"`,
    // not `"closed"` — verified directly against Node, which does not flip
    // it to `"closed"` until BOTH halves end, and a never-connected socket
    // has ended neither.
    let ready_state = entry::make_string(context, "open");
    set_value(context, instance, "readyState", ready_state);
    set_num(context, instance, "timeout", 0.0);
    let write_fn = entry::make_callable(context, write_hook);
    set_value(context, instance, "_write", write_fn);
    let destroy_fn = entry::make_callable(context, destroy_hook);
    set_value(context, instance, "_destroy", destroy_fn);
}

/// `socket.connect(port[, host][, connectListener])` and
/// `socket.connect(options[, connectListener])` — the path (IPC) overload
/// (`docs/reference/node/net.md` §2) is refused; see the module doc's own
/// "not implemented" section and [`target`].
pub(super) extern "C" fn connect(_e: u64, this: u64, a: u64, b: u64, c: u64, _d: u64) -> u64 {
    registry::pump();
    let absent = entry::undefined_value();
    let Some((port, host, listener)) = target(a, b, c) else {
        // A refusal is a throw already REGISTERED plus a return — see
        // `validate`'s own doc. `this` rather than `undefined` because the
        // returned value is never read once a throw is pending, and the
        // chaining shape of every other exit from this function is `this`.
        return this;
    };
    if port == 0 {
        return this;
    }
    if listener != absent {
        let once_fn = entry::with_runtime(|context| entry::get_member(context, this, "once"));
        if once_fn != absent {
            let event = super::common::key("connect");
            entry::call(once_fn, this, event, listener, absent, absent);
        }
    }
    let id = registry::next_id();
    entry::with_runtime(|context| super::common::set_num(context, this, "__socketId", id as f64));
    registry::with_sockets(|table| {
        table.insert(id, SocketEntry { owner: std::thread::current().id(), instance: this, queue: Default::default(), stream: None, pending: Vec::new(), closed: false });
    });
    entry::with_runtime(|context| super::common::set_bool(context, this, "connecting", true));
    std::thread::spawn(move || {
        match TcpStream::connect((host.as_str(), port)) {
            Ok(stream) => {
                let local = stream.local_addr().map(|a| a.to_string()).unwrap_or_default();
                let remote = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default();
                let reader = stream.try_clone().ok();
                registry::with_sockets(|table| {
                    if let Some(entry) = table.get_mut(&id) {
                        entry.stream = Some(stream);
                        entry.queue.push_back(SocketEvent::Connected { local, remote });
                    }
                });
                if let Some(reader) = reader {
                    spawn_reader(id, reader);
                }
            }
            Err(error) => registry::with_sockets(|table| {
                if let Some(entry) = table.get_mut(&id) {
                    entry.queue.push_back(SocketEvent::ConnectFailed(error.to_string()));
                }
            }),
        }
    });
    this
}

/// Which of `connect`'s overloads was written, with every argument checked.
///
/// `None` means a refusal is already registered and the caller must return —
/// see [`super::validate`]'s doc for why that is the shape, and why nothing
/// here raises from inside a runtime borrow.
///
/// The order of the checks is Node's own and matters: `connect()` and
/// `connect({})` both answer `ERR_MISSING_ARGS` naming all three spellings,
/// because at that point no argument has been supplied to have a type wrong;
/// only once a `port` IS present does it become a candidate for
/// `ERR_INVALID_ARG_TYPE`/`ERR_SOCKET_BAD_PORT`.
fn target(a: u64, b: u64, c: u64) -> Option<(u16, String, u64)> {
    let absent = entry::undefined_value();
    if a == absent {
        crate::errors::missing_args(&["options", "port", "path"]);
        return None;
    }
    // Read once, under one borrow: which overload this is, and — if it is the
    // options form — every field the checks below need. Reading them one at a
    // time would be four borrows for one call, and a check between two of them
    // would be the nested borrow that aborts an `extern "C"` frame.
    let shape = entry::with_runtime(|context| {
        if entry::number_of(a).is_some() {
            return Shape::Port;
        }
        if let Some(text) = entry::string_in(context, a) {
            return Shape::Text(text);
        }
        if !entry::is_object(context, a) {
            // A boolean, `null`, a symbol: Node reads a non-object first
            // argument as the port and lets `validatePort` name what is wrong
            // with it, which is why this is not its own refusal here.
            return Shape::Port;
        }
        let port = entry::get_member(context, a, "port");
        let path = entry::get_member(context, a, "path");
        let hints = entry::get_member(context, a, "hints");
        // Two statements and not one: `get_member` takes the context uniquely
        // and `string_in` takes it shared, so nesting the calls is a borrow
        // conflict rather than a style choice.
        let named = entry::get_member(context, a, "host");
        let host = entry::string_in(context, named);
        Shape::Options { port, path, hints, host }
    });
    match shape {
        Shape::Options { port, path, hints, host } => {
            if !super::validate::stream_options(a) || !super::validate::hints(hints) {
                return None;
            }
            if path != absent {
                // IPC is not implemented (module doc), and the option is
                // refused by name rather than ignored — an ignored `path` would
                // silently connect somewhere else, or nowhere.
                crate::errors::invalid_arg_value("options.path", path, "is not supported");
                return None;
            }
            if port == absent {
                crate::errors::missing_args(&["options", "port", "path"]);
                return None;
            }
            let port = super::validate::port("options.port", port)?;
            Some((port, host.unwrap_or_else(|| "localhost".to_owned()), b))
        }
        // A string that reads as a number is a port; one that does not is a
        // pipe/socket PATH, which is Node's own `isPipeName` split. The path
        // form is refused for the reason the options form's `path` is.
        Shape::Text(text) if super::validate::port_text(&text).is_none() => {
            crate::errors::invalid_arg_value("path", a, "is not supported");
            None
        }
        Shape::Port | Shape::Text(_) => {
            let port = super::validate::port("port", a)?;
            // The type TEST, not `ToString`. `text_of` converts, so an absent
            // second argument answered `Some("undefined")` — `connect(port)`
            // then looked up a host by that name, failed to resolve, and emitted
            // an `'error'` nothing handled. The overload test below inherited the
            // same wrong answer, so `connect(port, listener)` lost its listener.
            let host = entry::with_runtime(|context| entry::string_in(context, b));
            let listener = if host.is_some() { c } else { b };
            Some((port, host.unwrap_or_else(|| "localhost".to_owned()), listener))
        }
    }
}

/// Which overload [`target`] is looking at, plus the option fields it read
/// while it still held the borrow.
enum Shape {
    Port,
    Text(String),
    Options { port: u64, path: u64, hints: u64, host: Option<String> },
}

/// Reads until EOF/error, pushing each chunk — never calls into JS itself;
/// see the module doc's "WHEN a listener runs" section.
fn spawn_reader(id: u64, mut stream: TcpStream) {
    std::thread::spawn(move || {
        let mut buffer = [0u8; 16384];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => {
                    registry::with_sockets(|table| {
                        if let Some(entry) = table.get_mut(&id) {
                            entry.queue.push_back(SocketEvent::End);
                        }
                    });
                    return;
                }
                Ok(n) => registry::with_sockets(|table| {
                    if let Some(entry) = table.get_mut(&id)
                        && !entry.closed
                    {
                        entry.queue.push_back(SocketEvent::Data(buffer[..n].to_vec()));
                    }
                }),
                Err(error) => {
                    registry::with_sockets(|table| {
                        if let Some(entry) = table.get_mut(&id) {
                            entry.queue.push_back(SocketEvent::Error(error.to_string()));
                            entry.queue.push_back(SocketEvent::Closed { had_error: true });
                        }
                    });
                    return;
                }
            }
        }
    });
}

/// Builds a `Socket` for an already-accepted connection — `server.rs`'s own
/// [`super::server`] calls this from inside [`registry::pump`], never from
/// the accept thread itself.
pub(super) fn adopt(stream: TcpStream, remote: String) -> u64 {
    let local = stream.local_addr().map(|a| a.to_string()).unwrap_or_default();
    let id = registry::next_id();
    let reader = stream.try_clone().ok();
    let instance = entry::with_runtime(|context| {
        let prototype = prototype(context);
        let instance = entry::make_instance(context, prototype);
        init(context, instance);
        super::common::set_num(context, instance, "__socketId", id as f64);
        super::common::set_bool(context, instance, "connecting", false);
        super::common::set_bool(context, instance, "pending", false);
        let local_v = entry::make_string(context, &local);
        let remote_v = entry::make_string(context, &remote);
        entry::put_member(context, instance, "localAddress", local_v);
        entry::put_member(context, instance, "remoteAddress", remote_v);
        instance
    });
    registry::with_sockets(|table| {
        table.insert(id, SocketEntry { owner: std::thread::current().id(), instance, queue: Default::default(), stream: Some(stream), pending: Vec::new(), closed: false });
    });
    if let Some(reader) = reader {
        spawn_reader(id, reader);
    }
    instance
}

fn socket_id(this: u64) -> Option<u64> {
    let value = entry::get_indexed(this, super::common::key("__socketId"));
    entry::number_of(value).map(|v| v as u64)
}

/// Installed as `this._write` — see the module doc.
extern "C" fn write_hook(_e: u64, this: u64, chunk: u64, _encoding: u64, callback: u64, _d: u64) -> u64 {
    registry::pump();
    let absent = entry::undefined_value();
    let bytes = entry::text_of(chunk)
        .and_then(|text| entry::encode_text(&text, "utf8"))
        .or_else(|| entry::with_runtime(|context| entry::bytes_of(context, chunk)))
        .unwrap_or_default();
    let Some(id) = socket_id(this) else {
        entry::call(callback, absent, absent, absent, absent, absent);
        return absent;
    };
    let result = registry::write_now(id, &bytes);
    match result {
        Ok(()) => {
            let written = super::common::get_num(this, "bytesWritten") + bytes.len() as f64;
            entry::with_runtime(|context| super::common::set_num(context, this, "bytesWritten", written));
            entry::call(callback, absent, absent, absent, absent, absent);
        }
        Err(error) => {
            let text = error.to_string();
            let message = entry::with_runtime(|context| entry::make_string(context, &text));
            entry::call(callback, absent, message, absent, absent, absent);
        }
    }
    absent
}

/// Installed as `this._destroy` — closes the OS socket; the JS-side
/// bookkeeping (`destroyed`, `'close'`) is `readable::destroy`'s, inherited.
extern "C" fn destroy_hook(_e: u64, this: u64, _error: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if let Some(id) = socket_id(this) {
        registry::with_sockets(|table| {
            if let Some(entry) = table.get_mut(&id) {
                entry.closed = true;
                if let Some(stream) = &entry.stream {
                    let _ = stream.shutdown(std::net::Shutdown::Both);
                }
            }
        });
    }
    entry::undefined_value()
}

/// `socket.address()`.
extern "C" fn address(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let remote = super::common::get_value(this, "remoteAddress");
    entry::with_runtime(|context| {
        let object = entry::make_object(context);
        entry::put_member(context, object, "address", remote);
        object
    })
}

/// `socket.setTimeout(timeout, callback?)` — recorded, never enforced; no
/// idle-timer mechanism exists here (the same gap `fs/watch.rs` names for
/// anything that would need one). Named rather than silently honored.
///
/// The ARGUMENTS are checked even though the value is not acted on, and that is
/// deliberate: a program that writes `setTimeout('100')` has a bug Node reports
/// and we would otherwise store as the number `0`, so the gap in enforcement
/// would have been compounded by a gap in reporting.
extern "C" fn set_timeout(_e: u64, this: u64, timeout: u64, callback: u64, _c: u64, _d: u64) -> u64 {
    let Some(ms) = super::validate::msecs("msecs", timeout) else {
        return this;
    };
    if !super::validate::optional_callback("callback", callback) {
        return this;
    }
    entry::with_runtime(|context| super::common::set_num(context, this, "timeout", ms));
    let absent = entry::undefined_value();
    if callback != absent {
        let once_fn = entry::with_runtime(|context| entry::get_member(context, this, "once"));
        if once_fn != absent {
            let event = super::common::key("timeout");
            entry::call(once_fn, this, event, callback, absent, absent);
        }
    }
    this
}

/// `socket.setNoDelay(noDelay?)`.
extern "C" fn set_no_delay(_e: u64, this: u64, no_delay: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let flag = if no_delay == absent { true } else { entry::to_boolean(no_delay) };
    if let Some(id) = socket_id(this) {
        registry::with_sockets(|table| {
            if let Some(entry) = table.get(&id)
                && let Some(stream) = &entry.stream
            {
                let _ = stream.set_nodelay(flag);
            }
        });
    }
    this
}

/// `socket.setKeepAlive(enable?, initialDelay?)` — recorded, never applied
/// to the OS socket: `std::net::TcpStream` has no keepalive setter at all
/// (that needs a `setsockopt` crate this manifest does not carry — see
/// `Cargo.toml`'s own comment on what is vetted; adding one is outside this
/// module's five files). Named rather than silently ignored.
extern "C" fn set_keep_alive(_e: u64, this: u64, enable: u64, initial_delay: u64, _c: u64, _d: u64) -> u64 {
    let flag = entry::to_boolean(enable);
    let delay = entry::number_of(initial_delay).unwrap_or(0.0);
    entry::with_runtime(|context| {
        super::common::set_bool(context, this, "__keepAlive__", flag);
        super::common::set_num(context, this, "__keepAliveInitialDelay__", delay);
    });
    this
}

extern "C" fn noop_self(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    this
}
