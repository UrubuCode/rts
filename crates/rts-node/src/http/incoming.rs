//! `IncomingMessage` — a `Readable` — `docs/reference/node/http.md` §2.1.
//! [`outgoing`](super::outgoing) is the `OutgoingMessage`/`ServerResponse`
//! half; the two were one file until it passed the 500-line ceiling, and
//! this doc only states what is THIS file's.
//!
//! # Why this is built by hand instead of calling `Readable`'s own `construct`
//!
//! Exactly `net::socket`'s own reason: `stream::readable::init` is
//! `pub(super)`, scoped to `stream` alone. So [`readable_init`] sets the same
//! named data properties by hand (read directly off `stream/readable.rs`,
//! not guessed), and [`incoming_prototype`] chains onto `"Readable"` fetched
//! BY NAME — the SAME shared prototype `stream::namespace` already built.
//! Nothing about `push`/`read`/backpressure is reimplemented; only the
//! construction-time property list is duplicated, same as `net::socket`.
//!
//! # Header folding — `.headers` vs `.headersDistinct`
//!
//! [`folded_headers`] implements the exact rule `docs/reference/node/http.md`
//! §4 states: `set-cookie` becomes an array (never joined); a fixed list of
//! headers keeps only the first occurrence; everything else is joined with
//! `", "`. `.headersDistinct` is every value as an array, never joined or
//! dropped — built alongside rather than derived from the folded form, so
//! neither view can drift from the wire.

use rts_core::entry::{self, Context, Provided};

use super::common::*;
use super::parser::RawHeaders;

/// Header names Node keeps only the first value of when a message carries
/// duplicates — `docs/reference/node/http.md` §4.
const FIRST_ONLY: &[&str] = &[
    "age", "authorization", "content-length", "content-type", "etag", "expires", "from", "host",
    "if-modified-since", "if-unmodified-since", "last-modified", "location", "max-forwards",
    "proxy-authorization", "referer", "retry-after", "server", "user-agent",
];

const INCOMING_METHODS: &[(&str, Provided)] = &[("destroy", incoming_destroy), ("setTimeout", pub_set_timeout)];

pub(super) fn incoming_prototype(context: &mut Context) -> u64 {
    chained_prototype(context, "Readable", "IncomingMessage", INCOMING_METHODS)
}

/// Fields every `Readable` needs, set by hand — see the module doc.
fn readable_init(context: &mut Context, instance: u64) {
    init_emitter(context, instance);
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
    let buf = entry::make_array_in(context, Vec::new());
    entry::put_member(context, instance, "__buf__", buf);
    let pipes = entry::make_array_in(context, Vec::new());
    entry::put_member(context, instance, "__pipes__", pipes);
    let pipe_end = entry::make_array_in(context, Vec::new());
    entry::put_member(context, instance, "__pipeEnd__", pipe_end);
    set_bool(context, instance, "__ended__", false);
    set_bool(context, instance, "__emitClose__", true);
}

/// Builds an `IncomingMessage` — `server_side` picks which half of Node's
/// dual-purpose shape (server: `method`/`url`; client: `statusCode`/
/// `statusMessage`) gets populated; the other side's fields are simply not
/// set, matching Node leaving them `undefined`.
pub(super) fn build_incoming(
    context: &mut Context,
    socket: u64,
    headers: &RawHeaders,
    version: &str,
    server_side: Option<(&str, &str)>,
    client_side: Option<(u16, &str)>,
) -> u64 {
    let prototype = incoming_prototype(context);
    let instance = entry::make_instance(context, prototype);
    readable_init(context, instance);
    set_bool(context, instance, "aborted", false);
    set_bool(context, instance, "complete", false);
    set_value(context, instance, "socket", socket);
    set_value(context, instance, "connection", socket);
    let (major, minor) = version.split_once('.').unwrap_or((version, "0"));
    set_text(context, instance, "httpVersion", version);
    set_num(context, instance, "httpVersionMajor", major.parse().unwrap_or(1.0));
    set_num(context, instance, "httpVersionMinor", minor.parse().unwrap_or(1.0));
    let (folded, distinct) = folded_headers(context, headers);
    set_value(context, instance, "headers", folded);
    set_value(context, instance, "headersDistinct", distinct);
    let raw = raw_headers_array(context, headers);
    set_value(context, instance, "rawHeaders", raw);
    let empty_trailers = entry::make_object(context);
    set_value(context, instance, "trailers", empty_trailers);
    let empty_raw_trailers = entry::make_array_in(context, Vec::new());
    set_value(context, instance, "rawTrailers", empty_raw_trailers);
    if let Some((method, url)) = server_side {
        set_text(context, instance, "method", method);
        set_text(context, instance, "url", url);
    }
    if let Some((status, reason)) = client_side {
        set_num(context, instance, "statusCode", status as f64);
        set_text(context, instance, "statusMessage", reason);
    }
    instance
}

/// The folded `headers` object and the never-folded `headersDistinct` object
/// — see the module doc's header-folding section.
fn folded_headers(context: &mut Context, headers: &RawHeaders) -> (u64, u64) {
    let folded = entry::make_object(context);
    let distinct = entry::make_object(context);
    let mut seen_first_only: Vec<String> = Vec::new();
    let mut folded_values: Vec<(String, String)> = Vec::new();
    let mut set_cookies: Vec<String> = Vec::new();
    let mut distinct_values: Vec<(String, Vec<String>)> = Vec::new();
    for (name, value) in headers {
        let lower = name.to_ascii_lowercase();
        match distinct_values.iter_mut().find(|(n, _)| *n == lower) {
            Some((_, values)) => values.push(value.clone()),
            None => distinct_values.push((lower.clone(), vec![value.clone()])),
        }
        if lower == "set-cookie" {
            set_cookies.push(value.clone());
            continue;
        }
        if FIRST_ONLY.contains(&lower.as_str()) {
            if seen_first_only.contains(&lower) {
                continue;
            }
            seen_first_only.push(lower.clone());
            folded_values.push((lower, value.clone()));
            continue;
        }
        match folded_values.iter_mut().find(|(n, _)| *n == lower) {
            Some((_, existing)) => {
                existing.push_str(", ");
                existing.push_str(value);
            }
            None => folded_values.push((lower, value.clone())),
        }
    }
    for (name, value) in folded_values {
        set_text(context, folded, &name, &value);
    }
    if !set_cookies.is_empty() {
        let values: Vec<u64> = set_cookies.iter().map(|v| entry::make_string(context, v)).collect();
        let array = entry::make_array_in(context, values);
        entry::put_member(context, folded, "set-cookie", array);
    }
    for (name, values) in distinct_values {
        let held: Vec<u64> = values.iter().map(|v| entry::make_string(context, v)).collect();
        let array = entry::make_array_in(context, held);
        entry::put_member(context, distinct, &name, array);
    }
    (folded, distinct)
}

/// Flat `[name, value, name, value, ...]`, original case and order — the
/// unfolded pair every folded view above is built alongside, never from.
fn raw_headers_array(context: &mut Context, headers: &RawHeaders) -> u64 {
    let mut flat = Vec::with_capacity(headers.len() * 2);
    for (name, value) in headers {
        flat.push(entry::make_string(context, name));
        flat.push(entry::make_string(context, value));
    }
    entry::make_array_in(context, flat)
}

extern "C" fn incoming_destroy(_e: u64, this: u64, _error: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        set_bool(context, this, "destroyed", true);
        set_bool(context, this, "readable", false);
    });
    emit(this, "close", entry::undefined_value(), entry::undefined_value(), entry::undefined_value());
    this
}

/// `message.setTimeout(msecs, callback?)` — recorded, never enforced; the
/// same honest gap `net::socket::set_timeout` states (no idle-timer
/// mechanism exists in this crate at all). `pub(super)` because
/// [`super::outgoing`] reuses it verbatim for `OutgoingMessage`.
pub(super) extern "C" fn pub_set_timeout(_e: u64, this: u64, _msecs: u64, callback: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    if callback != absent {
        let once_fn = entry::with_runtime(|context| entry::get_member(context, this, "once"));
        if once_fn != absent {
            entry::call(once_fn, this, key("timeout"), callback, absent, absent);
        }
    }
    this
}
