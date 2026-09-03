//! `OutgoingMessage`/`ServerResponse` — a `Writable` — the other half of
//! [`incoming`](super::incoming), split out at the same 500-line ceiling.
//! `docs/reference/node/http.md` §2.1.
//!
//! Built by hand for the same reason `incoming.rs`'s doc gives for
//! `IncomingMessage`: `stream::writable::init` is private to `stream`, so
//! [`writable_init`] duplicates its property list and [`server_response_prototype`]
//! chains onto `"Writable"` fetched by name.

use rts_core::entry::{self, Context, Provided};

use super::common::*;
use super::incoming::pub_set_timeout;

pub(super) const OUTGOING_METHODS: &[(&str, Provided)] = &[
    ("writeHead", write_head),
    ("setHeader", set_header),
    ("getHeader", get_header),
    ("getHeaders", get_headers),
    ("getHeaderNames", get_header_names),
    ("hasHeader", has_header),
    ("removeHeader", remove_header),
    ("write", outgoing_write),
    ("end", outgoing_end),
    ("cork", outgoing_cork),
    ("uncork", outgoing_uncork),
    ("flushHeaders", flush_headers),
    ("addTrailers", add_trailers),
    ("setTimeout", pub_set_timeout),
    ("destroy", outgoing_destroy),
];

pub(super) fn server_response_prototype(context: &mut Context) -> u64 {
    chained_prototype(context, "Writable", "ServerResponse", OUTGOING_METHODS)
}

/// Fields every `Writable` needs, set by hand — see the module doc.
fn writable_init(context: &mut Context, instance: u64) {
    init_emitter(context, instance);
    let null = entry::null_in(context);
    set_bool(context, instance, "writableObjectMode", false);
    set_num(context, instance, "writableHighWaterMark", 16384.0);
    set_num(context, instance, "writableLength", 0.0);
    set_num(context, instance, "writableCorked", 0.0);
    set_bool(context, instance, "writableEnded", false);
    set_bool(context, instance, "writableFinished", false);
    set_bool(context, instance, "writableNeedDrain", false);
    set_bool(context, instance, "writableAborted", false);
    set_bool(context, instance, "writable", true);
    set_bool(context, instance, "destroyed", false);
    set_bool(context, instance, "closed", false);
    set_value(context, instance, "errored", null);
    let wqueue = entry::make_array_in(context, Vec::new());
    entry::put_member(context, instance, "__wqueue__", wqueue);
}

/// Builds a `ServerResponse` bound to `socket` and `req`. `write`/`end` on it
/// (installed as `_write` below) send bytes straight to `socket.write`,
/// which is where the actual TCP send happens — see `server.rs`.
pub(super) fn build_server_response(context: &mut Context, socket: u64, req: u64) -> u64 {
    let prototype = server_response_prototype(context);
    let instance = entry::make_instance(context, prototype);
    writable_init(context, instance);
    set_value(context, instance, "socket", socket);
    set_value(context, instance, "connection", socket);
    set_value(context, instance, "req", req);
    set_num(context, instance, "statusCode", 200.0);
    set_text(context, instance, "statusMessage", "");
    set_bool(context, instance, "headersSent", false);
    set_bool(context, instance, "sendDate", true);
    set_bool(context, instance, "strictContentLength", false);
    let headers = entry::make_object(context);
    set_value(context, instance, "__headers__", headers);
    let order = entry::make_array_in(context, Vec::new());
    set_value(context, instance, "__headerOrder__", order);
    let write_fn = entry::make_callable(context, response_write_hook);
    set_value(context, instance, "_write", write_fn);
    let destroy_fn = entry::make_callable(context, response_destroy_hook);
    set_value(context, instance, "_destroy", destroy_fn);
    instance
}

fn lower_key(context: &mut Context, name: &str) -> String {
    let _ = context;
    name.to_ascii_lowercase()
}

/// `response.setHeader(name, value)`.
extern "C" fn set_header(_e: u64, this: u64, name: u64, value: u64, _c: u64, _d: u64) -> u64 {
    let Some(name_text) = entry::text_of(name) else { return this };
    let existed = entry::with_runtime(|context| {
        let lower = lower_key(context, &name_text);
        let headers = get_value_in(context, this, "__headers__");
        let existed = entry::get_member(context, headers, &lower) != entry::undefined_in(context);
        entry::put_member(context, headers, &lower, value);
        existed
    });
    if !existed {
        // Read (`get_array_of`) and rebuild in TWO separate borrows, not one:
        // see that function's own doc for why reading an array's elements from
        // inside a held `with_runtime` is not an option.
        let mut order = get_array_of(this, "__headerOrder__");
        entry::with_runtime(|context| {
            order.push(entry::make_string(context, &name_text));
            let order_v = entry::make_array_in(context, order);
            set_value(context, this, "__headerOrder__", order_v);
        });
    }
    this
}

/// The header-name insertion order, as text — the piece [`super::client`]
/// needs to serialize a `ClientRequest`'s headers with the same folding this
/// file uses for a `ServerResponse`.
pub(super) fn header_order(this: u64) -> Vec<String> {
    get_array_of(this, "__headerOrder__").into_iter().filter_map(entry::text_of).collect()
}

/// [`set_header`], callable from outside this module — `ClientRequest`'s own
/// construction seeds its headers through the same one path a program's own
/// `setHeader` call does, rather than writing to `__headers__` directly.
pub(super) fn set_header_pub(this: u64, name: u64, value: u64) -> u64 {
    set_header(0, this, name, value, 0, 0)
}

/// The elements of an array-valued field (`__headerOrder__`), ambient.
///
/// # The bug this replaced, and why the fix is not just "use `get_indexed`"
///
/// The previous version took `context: &mut Context` and read each element
/// with `entry::get_member(context, value, &i.to_string())` — the
/// NAMED-property reader, on a key that is never a name, since an array's
/// elements live beside the cell rather than under one. Every call answered
/// `undefined` for element zero and stopped there (the loop's own exit
/// condition), so [`send_head_if_needed`] — what actually puts a
/// `ServerResponse`'s headers on the wire — read an ALWAYS-EMPTY order and
/// sent NONE of a caller's `setHeader` calls, silently, with `headersSent`
/// still flipping `true`; the same emptiness made [`header_order`] answer
/// nothing for a `ClientRequest`'s own header line (`http/client.rs`'s
/// `client_end`), and made [`remove_header`]'s pruning a no-op that quietly
/// replaced the order with an empty array on every call.
///
/// The element reader that sees array elements, `entry::get_indexed`, is
/// AMBIENT: it opens its own borrow of the thread-local context, and every
/// one of this function's four callers already holds one via
/// `entry::with_runtime`/a `context: &mut Context` parameter for its OWN
/// other work. Taking `context` here and calling `get_indexed` from inside it
/// would trade the silent empty-order bug for a `RefCell already borrowed`
/// abort — `common.rs`'s `get_value_in` names the identical hazard for the
/// same reason. So this reads nothing through a borrow it did not open
/// itself, and every caller now calls it from OUTSIDE its own.
fn get_array_of(this: u64, name: &str) -> Vec<u64> {
    let value = get_value(this, name);
    let count = entry::with_runtime(|context| {
        let length = entry::get_member(context, value, "length");
        entry::number_of(length).unwrap_or(0.0).max(0.0) as usize
    });
    (0..count).map(|index| entry::get_indexed(value, entry::make_number(index as f64))).collect()
}

/// `response.getHeader(name)`.
extern "C" fn get_header(_e: u64, this: u64, name: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(name_text) = entry::text_of(name) else { return entry::undefined_value() };
    entry::with_runtime(|context| {
        let lower = lower_key(context, &name_text);
        let headers = get_value_in(context, this, "__headers__");
        entry::get_member(context, headers, &lower)
    })
}

extern "C" fn get_headers(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    get_value(this, "__headers__")
}

extern "C" fn get_header_names(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    get_value(this, "__headerOrder__")
}

extern "C" fn has_header(_e: u64, this: u64, name: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(name_text) = entry::text_of(name) else { return entry::boolean_value(false) };
    entry::with_runtime(|context| {
        let lower = lower_key(context, &name_text);
        let headers = get_value_in(context, this, "__headers__");
        entry::boolean_value(entry::get_member(context, headers, &lower) != entry::undefined_in(context))
    })
}

extern "C" fn remove_header(_e: u64, this: u64, name: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(name_text) = entry::text_of(name) else { return entry::undefined_value() };
    let lower = entry::with_runtime(|context| {
        let lower = lower_key(context, &name_text);
        let headers = get_value_in(context, this, "__headers__");
        let absent = entry::undefined_in(context);
        entry::put_member(context, headers, &lower, absent);
        lower
    });
    // Outside the borrow above — see `get_array_of`'s doc.
    let mut order = get_array_of(this, "__headerOrder__");
    order.retain(|v| entry::text_of(*v).map(|t| t.to_ascii_lowercase()) != Some(lower.clone()));
    entry::with_runtime(|context| {
        let order_v = entry::make_array_in(context, order);
        set_value(context, this, "__headerOrder__", order_v);
    });
    entry::undefined_value()
}

/// `response.writeHead(statusCode[, statusMessage][, headers])`.
extern "C" fn write_head(_e: u64, this: u64, status: u64, b: u64, c: u64, _d: u64) -> u64 {
    if let Some(code) = entry::number_of(status) {
        entry::with_runtime(|context| set_num(context, this, "statusCode", code));
    }
    let absent = entry::undefined_value();
    let (message, headers) = match entry::text_of(b) {
        Some(text) => (Some(text), c),
        None => (None, b),
    };
    if let Some(message) = message {
        entry::with_runtime(|context| set_text(context, this, "statusMessage", &message));
    }
    if headers != absent {
        apply_headers_object(this, headers);
    }
    this
}

/// Copies own-named fields off a plain `{name: value}` object into
/// `__headers__`/`__headerOrder__` — an iterable of `[name, value]` pairs
/// (Node's other accepted shape) is not read; see the module's "not
/// implemented" section.
fn apply_headers_object(this: u64, headers: u64) {
    // `own_keys` answers a real array (its elements, not a shape of named
    // properties) — read the same way `get_array_of` reads `__headerOrder__`,
    // for the same reason: `get_indexed` is ambient and this loop must not
    // call it from inside a held borrow.
    let names = entry::own_keys(headers);
    let count = entry::with_runtime(|context| {
        let length = entry::get_member(context, names, "length");
        entry::number_of(length).unwrap_or(0.0).max(0.0) as usize
    });
    for index in 0..count {
        let name_value = entry::get_indexed(names, entry::make_number(index as f64));
        let Some(name) = entry::text_of(name_value) else { continue };
        let value = entry::with_runtime(|context| entry::get_member(context, headers, &name));
        set_header(0, this, name_value, value, 0, 0);
    }
}

extern "C" fn flush_headers(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    send_head_if_needed(this);
    entry::undefined_value()
}

extern "C" fn add_trailers(_e: u64, _this: u64, _headers: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    // Not implemented — see the module's "not implemented" section: no
    // chunked-trailer WRITE path exists, only the decode side (parser.rs).
    entry::undefined_value()
}

/// Serializes the status line + headers once, the first time a body write
/// (or an explicit `flushHeaders`) needs them on the wire. Chooses
/// `Transfer-Encoding: chunked` when the caller never set `Content-Length` —
/// `docs/reference/node/http.md` §4's stated default.
fn send_head_if_needed(this: u64) {
    if get_bool_of(this, "headersSent") {
        return;
    }
    entry::with_runtime(|context| set_bool(context, this, "headersSent", true));
    let status = get_num(this, "statusCode") as u16;
    let reason = get_text(this, "statusMessage").filter(|s| !s.is_empty()).unwrap_or_else(|| super::status::reason_phrase(status).to_owned());
    let mut head = format!("HTTP/1.1 {status} {reason}\r\n");
    let order = get_array_of(this, "__headerOrder__");
    let names: Vec<String> = order.into_iter().filter_map(entry::text_of).collect();
    let has_length = names.iter().any(|n| n.eq_ignore_ascii_case("content-length"));
    let headers_obj = get_value(this, "__headers__");
    for name in &names {
        let lower = name.to_ascii_lowercase();
        let value = entry::with_runtime(|context| entry::get_member(context, headers_obj, &lower));
        if let Some(text) = entry::text_of(value) {
            head.push_str(&format!("{name}: {text}\r\n"));
        }
    }
    let chunked = !has_length;
    if chunked {
        head.push_str("Transfer-Encoding: chunked\r\n");
    }
    head.push_str("Connection: close\r\n\r\n");
    entry::with_runtime(|context| set_bool(context, this, "__chunked__", chunked));
    let socket = get_value(this, "socket");
    let head_value = entry::with_runtime(|context| entry::make_string(context, &head));
    let absent = entry::undefined_value();
    call_method(socket, "write", head_value, absent, absent);
}

fn get_bool_of(this: u64, name: &str) -> bool {
    get_value(this, name) == entry::boolean_value(true)
}

/// Installed as `this._write` on a `ServerResponse` — sends the head (once)
/// then the chunk, framed per `__chunked__`.
extern "C" fn response_write_hook(_e: u64, this: u64, chunk: u64, _encoding: u64, callback: u64, _d: u64) -> u64 {
    send_head_if_needed(this);
    let bytes = entry::text_of(chunk)
        .and_then(|text| entry::encode_text(&text, "utf8"))
        .or_else(|| entry::with_runtime(|context| entry::bytes_of(context, chunk)))
        .unwrap_or_default();
    let socket = get_value(this, "socket");
    let absent = entry::undefined_value();
    let chunked = get_bool_of(this, "__chunked__");
    let payload = if chunked { chunk_frame(&bytes) } else { bytes };
    let payload_value = entry::with_runtime(|context| entry::make_bytes(context, &payload));
    call_method(socket, "write", payload_value, absent, absent);
    entry::call(callback, absent, absent, absent, absent, absent);
    absent
}

fn chunk_frame(bytes: &[u8]) -> Vec<u8> {
    let mut out = format!("{:x}\r\n", bytes.len()).into_bytes();
    out.extend_from_slice(bytes);
    out.extend_from_slice(b"\r\n");
    out
}

extern "C" fn response_destroy_hook(_e: u64, this: u64, _error: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let socket = get_value(this, "socket");
    call_method(socket, "destroy", entry::undefined_value(), entry::undefined_value(), entry::undefined_value());
    entry::undefined_value()
}

/// `writable.write(chunk, encoding?, callback?)` — the exact collapse
/// `stream::writable::write` makes, duplicated because that function is
/// private to `stream`.
extern "C" fn outgoing_write(_e: u64, this: u64, chunk: u64, encoding: u64, callback: u64, d: u64) -> u64 {
    let hook = entry::with_runtime(|context| entry::get_member(context, this, "_write"));
    entry::call(hook, this, chunk, encoding, callback, d);
    entry::boolean_value(true)
}

/// `writable.end(chunk?, encoding?, callback?)`.
extern "C" fn outgoing_end(_e: u64, this: u64, chunk: u64, encoding: u64, callback: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let (chunk, encoding, callback) = match chunk != absent && is_callable(chunk) {
        true => (absent, absent, chunk),
        false => match encoding != absent && is_callable(encoding) {
            true => (chunk, absent, encoding),
            false => (chunk, encoding, callback),
        },
    };
    send_head_if_needed(this);
    if chunk != absent {
        outgoing_write(0, this, chunk, encoding, absent, 0);
    }
    if get_bool_of(this, "__chunked__") {
        let terminator = entry::with_runtime(|context| entry::make_bytes(context, b"0\r\n\r\n"));
        let socket = get_value(this, "socket");
        call_method(socket, "write", terminator, absent, absent);
    }
    entry::with_runtime(|context| {
        set_bool(context, this, "writableEnded", true);
        set_bool(context, this, "writableFinished", true);
    });
    emit(this, "finish", absent, absent, absent);
    let socket = get_value(this, "socket");
    call_method(socket, "end", absent, absent, absent);
    if callback != absent {
        entry::call(callback, absent, absent, absent, absent, absent);
    }
    this
}

extern "C" fn outgoing_cork(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    this
}
extern "C" fn outgoing_uncork(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    this
}

extern "C" fn outgoing_destroy(_e: u64, this: u64, error: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| set_bool(context, this, "destroyed", true));
    let hook = entry::with_runtime(|context| entry::get_member(context, this, "_destroy"));
    entry::call(hook, this, error, entry::undefined_value(), entry::undefined_value(), entry::undefined_value());
    this
}
