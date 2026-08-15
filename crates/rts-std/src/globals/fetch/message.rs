//! `Request` and `Response` — a method, a URL, a status, a header list and a
//! body, and the four ways to read that body.
//!
//! # Why one file for two classes
//!
//! Because the Fetch Standard gives them one `Body` mixin and this is it:
//! `text()`, `json()`, `arrayBuffer()` and `bytes()` are defined once for both,
//! and so is `bodyUsed`. Splitting the classes would have meant either a third
//! file for four functions or two copies of them, and two copies of "what
//! reading a body does" is how the pair comes to disagree about `bodyUsed`.
//!
//! # Why the promises are already settled
//!
//! The body is resident: it was handed to the constructor. There is no
//! asynchrony to express, only the promise SHAPE the signature has, which is
//! exactly the case `entry::settled` documents — `node:buffer`'s `Blob` makes
//! the same trade for the same reason.
//!
//! # Not implemented, by name
//!
//! - **`json()` over a `Blob` body.** A `Blob`'s bytes live in `node:buffer`'s
//!   table, in a crate this one cannot depend on, and its readers are
//!   asynchronous — so `text()`/`arrayBuffer()`/`bytes()` FORWARD to the blob's
//!   own methods and keep working, while `json()` has nothing to parse
//!   synchronously and answers a rejected promise naming the reason. An absent
//!   answer, not a wrong one.
//! - **`FormData` as a body.** It would be serialized as `multipart/form-data`
//!   with a generated boundary, which is a writer this folder does not have.
//!   Such a body is stored and read back as `String(formData)`.
//! - **`request.clone()`/`response.clone()` sharing an unread stream.** The
//!   clone copies the body value, which is right for a resident body and would
//!   not be for a stream — the parent module names the missing stream.
//! - **`response.type`/`response.redirected`/`request.destination`** are the
//!   inert defaults a locally built message has (`"default"`, `false`, `""`).
//!   They only take other values through a network fetch.

use rts_core::entry::{self, Context, Provided};

const BODY: &str = "__body";

const RESPONSE_METHODS: &[(&str, Provided)] = &[
    ("text", text_method),
    ("json", json_method),
    ("arrayBuffer", array_buffer_method),
    ("bytes", bytes_method),
    ("clone", clone_response),
];

const REQUEST_METHODS: &[(&str, Provided)] = &[
    ("text", text_method),
    ("json", json_method),
    ("arrayBuffer", array_buffer_method),
    ("bytes", bytes_method),
    ("clone", clone_request),
];

/// The one `Request.prototype` / `Response.prototype`. Asked for HERE and
/// nowhere else — see [`super::class_of`] for what a second file asking cost.
fn request_prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "Request", REQUEST_METHODS)
}

fn response_prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "Response", RESPONSE_METHODS)
}

/// The `Request` and `Response` constructors.
pub(super) fn classes(context: &mut Context) -> (u64, u64) {
    let prototype = request_prototype(context);
    let request = super::class_of(context, "Request", prototype, construct_request);
    let prototype = response_prototype(context);
    let response = super::class_of(context, "Response", prototype, construct_response);
    let json_static = entry::make_callable(context, response_json);
    entry::put_member(context, response, "json", json_static);
    let error_static = entry::make_callable(context, response_error);
    entry::put_member(context, response, "error", error_static);
    let redirect_static = entry::make_callable(context, response_redirect);
    entry::put_member(context, response, "redirect", redirect_static);
    (request, response)
}

// ------------------------------------------------------------------- Response

/// `new Response(body?, init?)`.
extern "C" fn construct_response(_e: u64, this: u64, body: u64, init: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let status = option(init, "status").and_then(entry::number_of).unwrap_or(200.0);
    let status_text = option(init, "statusText").and_then(super::text).unwrap_or_default();
    let headers = super::headers::made(option_or(init, "headers", absent));
    let instance = entry::with_runtime(|context| {
        let prototype = response_prototype(context);
        let instance = super::self_or_new(context, this, prototype);
        let held = entry::make_number(status);
        entry::put_member(context, instance, "status", held);
        let text = entry::make_string(context, &status_text);
        entry::put_member(context, instance, "statusText", text);
        let ok = entry::boolean_value((200.0..300.0).contains(&status));
        entry::put_member(context, instance, "ok", ok);
        entry::put_member(context, instance, "headers", headers);
        entry::put_member(context, instance, "redirected", entry::boolean_value(false));
        let kind = entry::make_string(context, "default");
        entry::put_member(context, instance, "type", kind);
        let url = entry::make_string(context, "");
        entry::put_member(context, instance, "url", url);
        instance
    });
    attach_body(instance, body, headers);
    instance
}

/// `Response.json(data, init?)` — the body is the serialization, and the
/// `Content-Type` is the one the standard names for it.
extern "C" fn response_json(_e: u64, _this: u64, data: u64, init: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let Some(text) = stringify(data) else {
        return absent;
    };
    let body = super::string(&text);
    let made = entry::construct(class_named("Response"), body, init, absent, absent);
    let headers = entry::get_indexed(made, super::string("headers"));
    if super::headers::value_of(headers, "content-type").is_none() {
        super::headers::put(headers, "content-type", "application/json");
    }
    made
}

/// `Response.error()` — a network-error response, which is what a failed fetch
/// answers. Inert here in the way the module doc names, and correct in the two
/// things a program tests: `status === 0` and `type === "error"`.
extern "C" fn response_error(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let made = entry::construct(class_named("Response"), absent, absent, absent, absent);
    entry::with_runtime(|context| {
        let zero = entry::make_number(0.0);
        entry::put_member(context, made, "status", zero);
        entry::put_member(context, made, "ok", entry::boolean_value(false));
        let kind = entry::make_string(context, "error");
        entry::put_member(context, made, "type", kind);
        made
    })
}

/// `Response.redirect(url, status?)`.
extern "C" fn response_redirect(_e: u64, _this: u64, url: u64, status: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let code = entry::number_of(status).unwrap_or(302.0);
    let made = entry::construct(class_named("Response"), absent, absent, absent, absent);
    let location = super::text(url).unwrap_or_default();
    entry::with_runtime(|context| {
        let held = entry::make_number(code);
        entry::put_member(context, made, "status", held);
        entry::put_member(context, made, "ok", entry::boolean_value(false));
    });
    let headers = entry::get_indexed(made, super::string("headers"));
    super::headers::put(headers, "location", &location);
    made
}

// -------------------------------------------------------------------- Request

/// `new Request(input, init?)` — `input` is a URL string, a `URL` or another
/// `Request`, whose method, headers and body are the defaults.
extern "C" fn construct_request(_e: u64, this: u64, input: u64, init: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let from_request = entry::get_indexed(input, super::string("url"));
    let url = match from_request == absent {
        true => absolute(&super::text(input).unwrap_or_default()),
        false => super::text(from_request).unwrap_or_default(),
    };
    let method = option(init, "method")
        .and_then(super::text)
        .or_else(|| super::text(entry::get_indexed(input, super::string("method"))))
        .map_or_else(|| "GET".to_owned(), |asked| normalized_method(&asked));
    let inherited = entry::get_indexed(input, super::string("headers"));
    let headers = super::headers::made(option_or(init, "headers", inherited));
    let body = option_or(init, "body", entry::get_indexed(input, super::string(BODY)));
    let instance = entry::with_runtime(|context| {
        let prototype = request_prototype(context);
        let instance = super::self_or_new(context, this, prototype);
        let held = entry::make_string(context, &url);
        entry::put_member(context, instance, "url", held);
        let held = entry::make_string(context, &method);
        entry::put_member(context, instance, "method", held);
        entry::put_member(context, instance, "headers", headers);
        let held = entry::make_string(context, "");
        entry::put_member(context, instance, "destination", held);
        let held = entry::make_string(context, "follow");
        entry::put_member(context, instance, "redirect", held);
        instance
    });
    attach_body(instance, body, headers);
    instance
}

/// The standard uppercases exactly six method names and leaves every other one
/// alone, which is why this is a list and not `to_ascii_uppercase`: a program
/// sending `patch` gets `patch` in Node too.
fn normalized_method(asked: &str) -> String {
    const NORMALIZED: &[&str] = &["DELETE", "GET", "HEAD", "OPTIONS", "POST", "PUT"];
    let upper = asked.to_ascii_uppercase();
    match NORMALIZED.contains(&upper.as_str()) {
        true => upper,
        false => asked.to_owned(),
    }
}

/// A request's URL, serialized through the one `URL` in this workspace.
///
/// Reached through the global rather than parsed here: `node:url` owns the
/// WHATWG parser, and a second normalization would be a second answer to what a
/// URL is. Unparseable input is kept verbatim, which is where Node throws — the
/// same stand-in `node:url`'s own constructor takes.
fn absolute(text: &str) -> String {
    let class = super::global("URL");
    if !entry::with_runtime(|context| entry::is_callable_in(context, class)) {
        return text.to_owned();
    }
    let absent = entry::undefined_value();
    let made = entry::construct(class, super::string(text), absent, absent, absent);
    super::text(entry::get_indexed(made, super::string("href")))
        .filter(|href| !href.is_empty())
        .unwrap_or_else(|| text.to_owned())
}

// ----------------------------------------------------------------------- body

/// Records the body and the `Content-Type` it implies.
///
/// The header is written only when the list does not already carry one, which
/// is the standard's rule: an explicit `Content-Type` in `init.headers` wins
/// over the one a body kind suggests.
fn attach_body(instance: u64, body: u64, headers: u64) {
    let absent = entry::undefined_value();
    if body == absent || body == entry::null_value() {
        entry::with_runtime(|context| {
            entry::put_member(context, instance, "bodyUsed", entry::boolean_value(false))
        });
        return;
    }
    let (stored, kind) = normalized_body(body);
    entry::with_runtime(|context| {
        entry::put_member(context, instance, BODY, stored);
        entry::put_member(context, instance, "bodyUsed", entry::boolean_value(false));
    });
    if let Some(kind) = kind
        && super::headers::value_of(headers, "content-type").is_none()
    {
        super::headers::put(headers, "content-type", &kind);
    }
}

/// The value a body argument is stored as, and the `Content-Type` it implies.
fn normalized_body(body: u64) -> (u64, Option<String>) {
    if entry::with_runtime(|context| entry::string_in(context, body)).is_some() {
        return (body, Some("text/plain;charset=UTF-8".to_owned()));
    }
    if entry::with_runtime(|context| !entry::is_object(context, body)) {
        return (super::string(&super::text(body).unwrap_or_default()), None);
    }
    // A view over bytes goes in as it arrived, with no type — which is what the
    // standard says for a `BufferSource`.
    if entry::with_runtime(|context| entry::bytes_of(context, body)).is_some() {
        return (body, None);
    }
    // A `Blob` carries its own MIME type, and that IS the `Content-Type`.
    let mime = super::text(entry::get_indexed(body, super::string("type")));
    if entry::get_indexed(body, super::string("size")) != entry::undefined_value() {
        return (body, mime.filter(|mime| !mime.is_empty()));
    }
    // A `URLSearchParams` — recognised by the method the standard's own
    // serialization calls, not by a class name this crate cannot see.
    let serializer = entry::get_indexed(body, super::string("toString"));
    if entry::with_runtime(|context| entry::is_callable_in(context, serializer))
        && entry::get_indexed(body, super::string("getAll")) != entry::undefined_value()
    {
        let absent = entry::undefined_value();
        let text = entry::call(serializer, body, absent, absent, absent, absent);
        return (
            text,
            Some("application/x-www-form-urlencoded;charset=UTF-8".to_owned()),
        );
    }
    (super::string(&super::text(body).unwrap_or_default()), None)
}

/// The body of a message, and a rejected promise once it has been read.
///
/// `None` means the caller must answer the promise this already built: either
/// the "already used" rejection or the "no body" empty answer.
fn take_body(this: u64) -> Result<u64, u64> {
    let truth = entry::boolean_value(true);
    if entry::get_indexed(this, super::string("bodyUsed")) == truth {
        return Err(refuse("Body has already been consumed"));
    }
    entry::with_runtime(|context| entry::put_member(context, this, "bodyUsed", truth));
    Ok(entry::get_indexed(this, super::string(BODY)))
}

/// A promise rejected with the program's own `TypeError`.
fn rejected(message: u64) -> u64 {
    entry::with_runtime(|context| entry::settled(context, message, true))
}

/// The same, over a message this file writes.
fn refuse(message: &str) -> u64 {
    let error = entry::make_named_error("TypeError", message).unwrap_or_else(entry::undefined_value);
    rejected(error)
}

/// The body as text, when it is something this crate can read synchronously.
fn body_text(body: u64) -> Option<String> {
    if let Some(text) = entry::with_runtime(|context| entry::string_in(context, body)) {
        return Some(text);
    }
    let bytes = entry::with_runtime(|context| entry::bytes_of(context, body))?;
    Some(entry::decode_bytes(&bytes, "utf8"))
}

/// The body as bytes, on the same terms.
fn body_bytes(body: u64) -> Option<Vec<u8>> {
    if let Some(text) = entry::with_runtime(|context| entry::string_in(context, body)) {
        return entry::encode_text(&text, "utf8");
    }
    entry::with_runtime(|context| entry::bytes_of(context, body))
}

/// Forwards to the body's own reader, for a `Blob` this crate cannot read.
///
/// `None` when the body is not one — every caller then answers for itself.
fn forwarded(body: u64, name: &str) -> Option<u64> {
    let method = entry::get_indexed(body, super::string(name));
    if !entry::with_runtime(|context| entry::is_callable_in(context, method)) {
        return None;
    }
    let absent = entry::undefined_value();
    Some(entry::call(method, body, absent, absent, absent, absent))
}

/// `text()` — a settled promise of the body as UTF-8 text.
extern "C" fn text_method(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let body = match take_body(this) {
        Ok(body) => body,
        Err(answer) => return answer,
    };
    if let Some(text) = body_text(body) {
        let held = super::string(&text);
        return entry::with_runtime(|context| entry::settled(context, held, false));
    }
    forwarded(body, "text").unwrap_or_else(|| {
        let empty = super::string("");
        entry::with_runtime(|context| entry::settled(context, empty, false))
    })
}

/// `json()` — the text, parsed by the program's own `JSON.parse`.
extern "C" fn json_method(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let body = match take_body(this) {
        Ok(body) => body,
        Err(answer) => return answer,
    };
    let Some(text) = body_text(body) else {
        return refuse("json() cannot read this body synchronously — see message.rs");
    };
    let Some(parsed) = parse(&text) else {
        return refuse(&format!("Unexpected token in JSON: {text}"));
    };
    entry::with_runtime(|context| entry::settled(context, parsed, false))
}

/// `bytes()` — a settled promise of a `Uint8Array`.
extern "C" fn bytes_method(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let body = match take_body(this) {
        Ok(body) => body,
        Err(answer) => return answer,
    };
    match body_bytes(body) {
        Some(bytes) => entry::with_runtime(|context| {
            let held = entry::make_bytes(context, &bytes);
            entry::settled(context, held, false)
        }),
        None => forwarded(body, "bytes").unwrap_or_else(|| refuse("this body has no bytes")),
    }
}

/// `arrayBuffer()` — the `ArrayBuffer` behind the same bytes, reached through a
/// fresh view's `buffer` rather than minted here, for the reason `node:buffer`'s
/// `Blob` records: there is no host entry that makes a bare `ArrayBuffer`.
extern "C" fn array_buffer_method(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let body = match take_body(this) {
        Ok(body) => body,
        Err(answer) => return answer,
    };
    match body_bytes(body) {
        Some(bytes) => entry::with_runtime(|context| {
            let view = entry::make_bytes(context, &bytes);
            let buffer = entry::get_member(context, view, "buffer");
            entry::settled(context, buffer, false)
        }),
        None => forwarded(body, "arrayBuffer").unwrap_or_else(|| refuse("this body has no bytes")),
    }
}

/// `response.clone()` / `request.clone()`.
extern "C" fn clone_response(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let body = entry::get_indexed(this, super::string(BODY));
    let init = entry::with_runtime(|context| {
        let init = entry::make_object(context);
        for name in ["status", "statusText", "headers"] {
            let held = entry::get_member(context, this, name);
            entry::put_member(context, init, name, held);
        }
        init
    });
    entry::construct(class_named("Response"), body, init, absent, absent)
}

extern "C" fn clone_request(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    entry::construct(class_named("Request"), this, absent, absent, absent)
}

// --------------------------------------------------------------------- shared

/// One field of an `init` bag, `None` when there is no bag or no field.
fn option(init: u64, name: &str) -> Option<u64> {
    let absent = entry::undefined_value();
    if init == absent || !entry::with_runtime(|context| entry::is_object(context, init)) {
        return None;
    }
    let held = entry::get_indexed(init, super::string(name));
    (held != absent).then_some(held)
}

/// The same, with a fallback.
fn option_or(init: u64, name: &str, fallback: u64) -> u64 {
    option(init, name).unwrap_or(fallback)
}

/// One of this folder's own classes, by the global it is bound to.
///
/// Through the global rather than a stored handle because `class_of` is
/// idempotent per context and this is the shortest route to the same cell — the
/// alternative, a process-global `static`, is wrong for a per-thread context.
fn class_named(name: &str) -> u64 {
    super::global(name)
}

/// `JSON.parse` and `JSON.stringify`, reached through the program's own `JSON`.
///
/// This crate has no JSON parser and must not grow one: `rts-core` owns the
/// only one, and a second would be a second answer to what `{"a":1}` means.
fn parse(text: &str) -> Option<u64> {
    json_call("parse", super::string(text))
}

fn stringify(value: u64) -> Option<String> {
    super::text(json_call("stringify", value)?)
}

fn json_call(name: &str, argument: u64) -> Option<u64> {
    let json = super::global("JSON");
    let method = entry::get_indexed(json, super::string(name));
    if !entry::with_runtime(|context| entry::is_callable_in(context, method)) {
        return None;
    }
    let absent = entry::undefined_value();
    Some(entry::call(method, json, argument, absent, absent, absent))
}
