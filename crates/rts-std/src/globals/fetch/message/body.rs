//! Reading a body that is already attached — `text()`, `json()`,
//! `arrayBuffer()`, `bytes()`, `blob()`, `clone()`, and what a body argument
//! is stored as in the first place.
//!
//! Split out of the parent module once it passed the 500-line ceiling; nothing
//! here decides what `Request`/`Response` themselves look like, only what a
//! body value already sitting under `[super::BODY]` answers when read.

use rts_core::entry;

use super::BODY;

/// Records the body and the `Content-Type` it implies.
///
/// The header is written only when the list does not already carry one, which
/// is the standard's rule: an explicit `Content-Type` in `init.headers` wins
/// over the one a body kind suggests.
pub(super) fn attach_body(instance: u64, body: u64, headers: u64) {
    let absent = entry::undefined_value();
    if body == absent || body == entry::null_value() {
        entry::with_runtime(|context| {
            entry::put_member(context, instance, "bodyUsed", entry::boolean_value(false));
            // `body === null` is what a program checks for "nothing to read" —
            // this engine has no `ReadableStream` to answer instead (the module
            // doc names the gap), so `null` is the honest answer rather than
            // `undefined`, which is what an unset property would read as.
            let none = entry::null_in(context);
            entry::put_member(context, instance, "body", none);
        });
        return;
    }
    let (stored, kind) = normalized_body(body);
    entry::with_runtime(|context| {
        entry::put_member(context, instance, BODY, stored);
        entry::put_member(context, instance, "bodyUsed", entry::boolean_value(false));
        // Non-null is the whole of what `body !== null` needs; a real
        // `ReadableStream` here is the same gap `attach_body`'s `null` branch
        // states, not a new one this line invents.
        entry::put_member(context, instance, "body", stored);
    });
    if let Some(kind) = kind
        && super::super::headers::value_of(headers, "content-type").is_none()
    {
        super::super::headers::put(headers, "content-type", &kind);
    }
}

/// The value a body argument is stored as, and the `Content-Type` it implies.
fn normalized_body(body: u64) -> (u64, Option<String>) {
    if entry::with_runtime(|context| entry::string_in(context, body)).is_some() {
        return (body, Some("text/plain;charset=UTF-8".to_owned()));
    }
    if entry::with_runtime(|context| !entry::is_object(context, body)) {
        return (super::super::string(&super::super::text(body).unwrap_or_default()), None);
    }
    // A view over bytes goes in as it arrived, with no type — which is what the
    // standard says for a `BufferSource`.
    if entry::with_runtime(|context| entry::buffer_source_bytes(context, body)).is_some() {
        return (body, None);
    }
    // A `URLSearchParams` — recognised by the method the standard's own
    // serialization calls, not by a class name this crate cannot see. Checked
    // BEFORE the `Blob` heuristic below: the WHATWG URL Standard gave
    // `URLSearchParams` its own `.size` getter (the parameter count), which
    // made it match "has a `size`" first and be read as an empty `Blob` — the
    // body went in unstringified, with no `Content-Type`, and every reader
    // downstream saw nothing to decode.
    let serializer = entry::get_indexed(body, super::super::string("toString"));
    if entry::with_runtime(|context| entry::is_callable_in(context, serializer))
        && entry::get_indexed(body, super::super::string("getAll")) != entry::undefined_value()
    {
        let absent = entry::undefined_value();
        let text = entry::call(serializer, body, absent, absent, absent, absent);
        return (
            text,
            Some("application/x-www-form-urlencoded;charset=UTF-8".to_owned()),
        );
    }
    // A `Blob` carries its own MIME type, and that IS the `Content-Type`.
    let mime = super::super::text(entry::get_indexed(body, super::super::string("type")));
    if entry::get_indexed(body, super::super::string("size")) != entry::undefined_value() {
        return (body, mime.filter(|mime| !mime.is_empty()));
    }
    (super::super::string(&super::super::text(body).unwrap_or_default()), None)
}

/// The body of a message, and a rejected promise once it has been read.
///
/// `None` means the caller must answer the promise this already built: either
/// the "already used" rejection or the "no body" empty answer.
fn take_body(this: u64) -> Result<u64, u64> {
    let truth = entry::boolean_value(true);
    if entry::get_indexed(this, super::super::string("bodyUsed")) == truth {
        return Err(refuse("Body has already been consumed"));
    }
    entry::with_runtime(|context| entry::put_member(context, this, "bodyUsed", truth));
    Ok(entry::get_indexed(this, super::super::string(BODY)))
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
    let bytes = entry::with_runtime(|context| entry::buffer_source_bytes(context, body))?;
    Some(entry::decode_bytes(&bytes, "utf8"))
}

/// The body as bytes, on the same terms.
fn body_bytes(body: u64) -> Option<Vec<u8>> {
    if let Some(text) = entry::with_runtime(|context| entry::string_in(context, body)) {
        return entry::encode_text(&text, "utf8");
    }
    entry::with_runtime(|context| entry::buffer_source_bytes(context, body))
}

/// Forwards to the body's own reader, for a `Blob` this crate cannot read.
///
/// `None` when the body is not one — every caller then answers for itself.
fn forwarded(body: u64, name: &str) -> Option<u64> {
    let method = entry::get_indexed(body, super::super::string(name));
    if !entry::with_runtime(|context| entry::is_callable_in(context, method)) {
        return None;
    }
    let absent = entry::undefined_value();
    Some(entry::call(method, body, absent, absent, absent, absent))
}

/// `text()` — a settled promise of the body as UTF-8 text.
pub(super) extern "C" fn text_method(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let body = match take_body(this) {
        Ok(body) => body,
        Err(answer) => return answer,
    };
    if let Some(text) = body_text(body) {
        let held = super::super::string(&text);
        return entry::with_runtime(|context| entry::settled(context, held, false));
    }
    forwarded(body, "text").unwrap_or_else(|| {
        let empty = super::super::string("");
        entry::with_runtime(|context| entry::settled(context, empty, false))
    })
}

/// `json()` — the text, parsed by the program's own `JSON.parse`.
pub(super) extern "C" fn json_method(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let body = match take_body(this) {
        Ok(body) => body,
        Err(answer) => return answer,
    };
    let Some(text) = body_text(body) else {
        return refuse("json() cannot read this body synchronously — see message/body.rs");
    };
    let Some(parsed) = parse(&text) else {
        return refuse(&format!("Unexpected token in JSON: {text}"));
    };
    entry::with_runtime(|context| entry::settled(context, parsed, false))
}

/// `bytes()` — a settled promise of a `Uint8Array`.
pub(super) extern "C" fn bytes_method(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
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
pub(super) extern "C" fn array_buffer_method(
    _e: u64,
    this: u64,
    _a: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
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

/// `blob()` — the body as a `node:buffer` `Blob`, tagged with the message's own
/// `Content-Type`.
///
/// Reached through the GLOBAL the same way [`super::super`]'s own doc names for
/// `File`/`Blob`: this crate cannot depend on `rts-node`, so a second `Blob`
/// class is the alternative, and `blob instanceof Blob` would then be `false`
/// for the very object this method built.
pub(super) extern "C" fn blob_method(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let body = match take_body(this) {
        Ok(body) => body,
        Err(answer) => return answer,
    };
    let headers = entry::get_indexed(this, super::super::string("headers"));
    let mime = super::super::headers::value_of(headers, "content-type").unwrap_or_default();
    // A body that already IS a Blob-shaped object (constructed from one, or
    // forwarded from another `blob()`) answers itself — re-wrapping its bytes
    // would drop whatever MIME type it already carries.
    if entry::get_indexed(body, super::super::string("size")) != entry::undefined_value() {
        return entry::with_runtime(|context| entry::settled(context, body, false));
    }
    let bytes = body_bytes(body).unwrap_or_default();
    let class = super::super::global("Blob");
    if !entry::with_runtime(|context| entry::is_callable_in(context, class)) {
        return refuse("node:buffer's Blob is not installed");
    }
    let made = entry::with_runtime(|context| {
        let view = entry::make_bytes(context, &bytes);
        let parts = entry::make_array_in(context, vec![view]);
        let options = entry::make_object(context);
        let mime_value = entry::make_string(context, &mime);
        entry::put_member(context, options, "type", mime_value);
        (parts, options)
    });
    let absent = entry::undefined_value();
    let blob = entry::construct(class, made.0, made.1, absent, absent);
    entry::with_runtime(|context| entry::settled(context, blob, false))
}

/// `response.clone()` / `request.clone()`.
pub(super) extern "C" fn clone_response(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let body = entry::get_indexed(this, super::super::string(BODY));
    let init = entry::with_runtime(|context| {
        let init = entry::make_object(context);
        for name in ["status", "statusText", "headers"] {
            let held = entry::get_member(context, this, name);
            entry::put_member(context, init, name, held);
        }
        init
    });
    entry::construct(super::class_named("Response"), body, init, absent, absent)
}

pub(super) extern "C" fn clone_request(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    entry::construct(super::class_named("Request"), this, absent, absent, absent)
}

/// `JSON.parse` and `JSON.stringify`, reached through the program's own `JSON`.
///
/// This crate has no JSON parser and must not grow one: `rts-core` owns the
/// only one, and a second would be a second answer to what `{"a":1}` means.
fn parse(text: &str) -> Option<u64> {
    json_call("parse", super::super::string(text))
}

pub(super) fn stringify(value: u64) -> Option<String> {
    super::super::text(json_call("stringify", value)?)
}

fn json_call(name: &str, argument: u64) -> Option<u64> {
    let json = super::super::global("JSON");
    let method = entry::get_indexed(json, super::super::string(name));
    if !entry::with_runtime(|context| entry::is_callable_in(context, method)) {
        return None;
    }
    let absent = entry::undefined_value();
    Some(entry::call(method, json, argument, absent, absent, absent))
}
