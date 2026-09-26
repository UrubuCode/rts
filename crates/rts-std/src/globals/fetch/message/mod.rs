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
//! # Why THIS split, into a folder
//!
//! `body` is everything about READING an already-attached body — the four
//! reader methods, `clone()`, and the JSON bridge they share — split out once
//! this file passed the 500-line ceiling `CLAUDE.md` sets for everything
//! outside the two engine crates. What stays here is the two classes
//! themselves: their prototypes, their constructors, and `Response`'s three
//! static factories, none of which reads a body.
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
//! - **A real `ReadableStream` for `.body`.** `Response.body`/`Request.body`
//!   answer `null` when there is no body and the stored value otherwise —
//!   enough for the `!== null` a program checks, not a stream it can read from.

mod body;

use rts_core::entry::{self, Context, Provided};

pub(super) const BODY: &str = "__body";

const RESPONSE_METHODS: &[(&str, Provided)] = &[
    ("text", body::text_method),
    ("json", body::json_method),
    ("arrayBuffer", body::array_buffer_method),
    ("bytes", body::bytes_method),
    ("blob", body::blob_method),
    ("clone", body::clone_response),
];

const REQUEST_METHODS: &[(&str, Provided)] = &[
    ("text", body::text_method),
    ("json", body::json_method),
    ("arrayBuffer", body::array_buffer_method),
    ("bytes", body::bytes_method),
    ("blob", body::blob_method),
    ("clone", body::clone_request),
];

/// The one `Request.prototype` / `Response.prototype`. Asked for HERE and
/// nowhere else — see [`super::class_of`] for what a second file asking cost.
fn request_prototype(context: &mut Context) -> u64 {
    let prototype = entry::make_prototype(context, "Request", REQUEST_METHODS);
    tagged(context, prototype, "Request")
}

fn response_prototype(context: &mut Context) -> u64 {
    let prototype = entry::make_prototype(context, "Response", RESPONSE_METHODS);
    tagged(context, prototype, "Response")
}

/// `Object.prototype.toString.call(x)` — the string-keyed `"@@toStringTag"`
/// convention every host class here uses for a symbol-keyed member (`Headers`,
/// `node:crypto`'s `webcrypto`, `TextDecoder`, DOM's `Event`).
fn tagged(context: &mut Context, prototype: u64, name: &str) -> u64 {
    let tag = entry::make_string(context, name);
    entry::put_member(context, prototype, "@@toStringTag", tag);
    prototype
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

/// The standard's window for `new Response(_, { status })` — 199 and 600 both
/// refused, everything between accepted whether or not it is a status any
/// server would send (a locally built `Response` never talks to one).
fn valid_response_status(status: f64) -> bool {
    (200.0..=599.0).contains(&status)
}

/// `new Response(body?, init?)`.
extern "C" fn construct_response(_e: u64, this: u64, body: u64, init: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let status = option(init, "status").and_then(entry::number_of).unwrap_or(200.0);
    if !valid_response_status(status) {
        return range_error(&format!(
            "Failed to construct 'Response': The status provided ({status}) is outside the range [200, 599]."
        ));
    }
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
    body::attach_body(instance, body, headers);
    instance
}

/// `Response.json(data, init?)` — the body is the serialization, and the
/// `Content-Type` is the one the standard names for it.
extern "C" fn response_json(_e: u64, _this: u64, data: u64, init: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let Some(text) = body::stringify(data) else {
        return absent;
    };
    // Whether the CALLER named a `Content-Type`, checked before the body is
    // attached: a plain string body implies `text/plain` (see
    // `body::normalized_body`), so by the time `made`'s own headers exist, ONE
    // is always already there — the implied one when the caller named none —
    // and `.is_none()` below would never fire. `replace` is what forces
    // `application/json` over that implied default without disturbing a
    // `Content-Type` the caller DID name.
    let user_named_content_type = option(init, "headers")
        .map(|raw| super::headers::value_of(super::headers::made(raw), "content-type").is_some())
        .unwrap_or(false);
    let body = super::string(&text);
    let made = entry::construct(class_named("Response"), body, init, absent, absent);
    let headers = entry::get_indexed(made, super::string("headers"));
    if !user_named_content_type {
        super::headers::replace(headers, "content-type", "application/json");
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

/// The statuses the standard's `Response.redirect` accepts — not the whole
/// 300–399 range, exactly these five.
const REDIRECT_STATUSES: &[f64] = &[301.0, 302.0, 303.0, 307.0, 308.0];

/// `Response.redirect(url, status?)`.
extern "C" fn response_redirect(_e: u64, _this: u64, url: u64, status: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let code = entry::number_of(status).unwrap_or(302.0);
    if !REDIRECT_STATUSES.contains(&code) {
        return range_error(&format!(
            "Failed to execute 'redirect' on 'Response': Invalid status code {code}"
        ));
    }
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
        true => match absolute(&super::text(input).unwrap_or_default()) {
            Some(url) => url,
            // `new Request("relative/path")` — no base to resolve against,
            // which is a `TypeError` the standard raises at construction
            // rather than a `Request` carrying a URL nothing can fetch.
            None => {
                entry::throw_type_error("Failed to construct 'Request': Invalid URL");
                return absent;
            }
        },
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
    body::attach_body(instance, body, headers);
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
/// URL is. A relative path with no base — `new Request("relative/path")` — is
/// exactly what `new URL(text)` refuses, so its refusal IS the one this
/// function forwards rather than a second check of its own.
///
/// `None` when `URL` is installed and REFUSED `text` — the caller's own
/// `TypeError` to raise, not this file's to swallow, since `absolute` is not a
/// constructor and cannot leave a throw behind silently discarded.
fn absolute(text: &str) -> Option<String> {
    let class = super::global("URL");
    if !entry::with_runtime(|context| entry::is_callable_in(context, class)) {
        return Some(text.to_owned());
    }
    let absent = entry::undefined_value();
    let made = entry::construct(class, super::string(text), absent, absent, absent);
    if entry::pending().is_some() {
        entry::take_thrown();
        return None;
    }
    let href = super::text(entry::get_indexed(made, super::string("href"))).unwrap_or_default();
    // An empty `href` with nothing thrown is `node:url`'s OWN way of failing a
    // relative input with no base — this file cannot tell that apart from a
    // genuine throw without reaching into that crate, so it is read the same
    // way: refused, not silently kept verbatim. A real absolute URL never
    // serializes to `""`.
    match href.is_empty() {
        true => None,
        false => Some(href),
    }
}

// --------------------------------------------------------------------- shared

/// One field of an `init` bag, `None` when there is no bag or no field.
pub(super) fn option(init: u64, name: &str) -> Option<u64> {
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
pub(super) fn class_named(name: &str) -> u64 {
    super::global(name)
}

/// Raises a catchable `RangeError` — the standard's answer for a `Response`
/// status outside 200–599 or a `redirect()` status that is not one of the five
/// it names. Thrown rather than settled into a rejected promise: both callers
/// are inside a CONSTRUCTOR/static factory, which the standard has fail
/// synchronously, the same way `new Array(-1)` does in every engine.
fn range_error(message: &str) -> u64 {
    match entry::make_named_error("RangeError", message) {
        Some(error) => {
            entry::throw_value(error);
            entry::undefined_value()
        }
        None => entry::undefined_value(),
    }
}

#[cfg(test)]
mod tests {
    use super::{REDIRECT_STATUSES, valid_response_status};

    #[test]
    fn response_status_window_is_200_through_599_inclusive() {
        assert!(!valid_response_status(199.0));
        assert!(valid_response_status(200.0));
        assert!(valid_response_status(204.0));
        assert!(valid_response_status(599.0));
        assert!(!valid_response_status(600.0));
        assert!(!valid_response_status(0.0));
        assert!(!valid_response_status(-1.0));
        assert!(!valid_response_status(1000.0));
    }

    #[test]
    fn redirect_accepts_exactly_the_five_named_statuses() {
        // The standard's set, not the whole 300–399 range: 300, 304 and 306
        // are none of them redirects `Response.redirect` may mint.
        for status in [301.0, 302.0, 303.0, 307.0, 308.0] {
            assert!(REDIRECT_STATUSES.contains(&status), "{status} should be accepted");
        }
        for status in [200.0, 300.0, 304.0, 306.0, 399.0] {
            assert!(!REDIRECT_STATUSES.contains(&status), "{status} should be refused");
        }
    }
}
