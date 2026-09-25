//! `fetch(url)` — the global a page uses to go get whatever it asks for.
//!
//! # Why this lives here and not in `rts-std`
//!
//! `fetch` is a BROWSER global, and a reasonable instinct is to put it with
//! the other globals. But it needs HTTP, and HTTP lives in this crate —
//! `http/` and `https/`, the latter with TLS. Putting it in `rts-std` would
//! force a second client, which is exactly what this repository's
//! `reuse-check` exists to prevent.
//!
//! And it is not a concession: Node 18+ has `fetch` as a global just as much
//! as a browser does. It belongs on this list for the same reason `Buffer`,
//! `process` and `URL` do — it is what this environment offers without an
//! import. A PAGE sees it because a browser has it too, which is why it is
//! not on the `NODE_ONLY` list `emit::globals` hides from a `<script>`.
//!
//! # What this `fetch` does and does NOT do
//!
//! Makes a request and answers a `Promise` of a `Response`, with `status`,
//! `ok`, `statusText`, `headers`, `text()` and `json()`. `http:` and
//! `https:` — the scheme picks the client.
//!
//! **It is SYNCHRONOUS underneath, and the promise arrives already settled.**
//! This crate's client reads the whole response before returning — its own
//! module explains why, and that is a decision from before this one. The
//! consequence is observable and stated: two `fetch` calls never overlap, and
//! a page making ten "parallel" requests pays for them in a queue. What does
//! NOT happen is the promise lying: once it resolves, the body is really
//! there.
//!
//! No `Request`, no `Headers` class, no body streaming, no `AbortSignal`, no
//! automatic redirects. Each of those is an answer this does not give yet,
//! and the absence is the honest way to say so — a `redirect: "follow"` that
//! did not follow would be worse than the lack of one.
//!
//! # Why a rejection now has a REASON
//!
//! A failure used to become a bare `TypeError: fetch: <url> falhou`, without
//! saying whether it was the URL, the connection, the TLS handshake or the
//! read — nor, on a read, how many bytes had already arrived.
//! [`request::request`] now returns `Result<Response, String>`, and the
//! reason travels into the `TypeError`'s message. `request.rs` has why each
//! function inside it returns its own reason instead of a `bool`/`Option`.

mod request;

use rts_core::entry::{self, Context};

use request::request;

/// How long to wait for the connection and for the response, in
/// milliseconds.
///
/// The same numbers `http::client` uses, for the same reason it has them: a
/// request that never answers cannot hold up the program forever.
const TIMEOUT_MS: u64 = 15_000;

/// `fetch(url)` and `fetch(url, options)`.
pub(crate) extern "C" fn fetch(
    _e: u64,
    _this: u64,
    url: u64,
    options: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    let Some(text) = entry::text_of(url) else {
        return reject("fetch: the URL must be text");
    };
    // The VALUES stay under the borrow, their TEXT comes OUT. `text_of`
    // enters the context on its own, and asking for it from inside aborts
    // the process instead of failing — the same rule this file already
    // states in three places, and one I went back and broke twice while
    // writing it. The underline is worth it: knowing the rule does not stop
    // it being broken; what stops it is the code saying which side is which.
    let (method, body) = entry::with_runtime(|context| {
        (
            entry::get_member(context, options, "method"),
            entry::get_member(context, options, "body"),
        )
    });
    // `text_of` of `undefined` answers the STRING `"undefined"`, and that was
    // what went in as the method: `fetch(url)` with no options built
    // `undefined / HTTP/1.1` with body `undefined`, and the server answered
    // 400 — which reads like a network problem and is a conversion happening
    // where nobody asked for it.
    let text_or = |value: u64, default: &str| -> String {
        match entry::text_of(value) {
            Some(t) if t != "undefined" && t != "null" => t,
            _ => default.to_owned(),
        }
    };
    let method = text_or(method, "GET");
    let body = text_or(body, "");
    let extra_headers = headers_of(options);

    match request(&text, &method, &body, &extra_headers) {
        Ok(response) => {
            let object = entry::with_runtime(|context| build_response(context, &text, response));
            entry::with_runtime(|context| entry::settled(context, object, false))
        }
        Err(reason) => reject(&format!("fetch: {text}: {reason}")),
    }
}

/// An already-REJECTED promise with a `TypeError`, which is what the spec says
/// for a `fetch` that never happens — a network that is down, an impossible
/// URL.
fn reject(why: &str) -> u64 {
    // The error is built OUTSIDE the borrow, and the promise inside it.
    // `make_named_error` enters the context on its own, and asking for it
    // from in here is a non-unwindable abort instead of an error — the same
    // rule the DOM's `scope.rs` and egui's `with_egui` had already written,
    // and one I went back and broke while writing this file.
    let error = entry::make_named_error("TypeError", why);
    entry::with_runtime(|context| {
        let error = error.unwrap_or_else(|| entry::undefined_in(context));
        entry::settled(context, error, true)
    })
}

/// What a response brings back.
pub(super) struct Response {
    status: i64,
    reason: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

/// The `headers` the program passed, as pairs.
///
/// A plain object, which is the shape `fetch(url, { headers: { … } })` uses
/// most. The `Headers` class does not exist here and so is not accepted —
/// saying so by its absence rather than faking it.
fn headers_of(options: u64) -> Vec<(String, String)> {
    let object = entry::with_runtime(|context| entry::get_member(context, options, "headers"));
    let names = entry::with_runtime(|context| entry::member_names(context, object));
    let mut out = Vec::new();
    for name in names {
        let value = entry::with_runtime(|context| entry::get_member(context, object, &name));
        if let Some(value) = entry::text_of(value) {
            out.push((name, value));
        }
    }
    out
}

/// The `Response` object the promise delivers.
///
/// Its methods answer promises, as in a browser — `res.text()` is always an
/// `await`, and a version that returned the raw string would make code
/// written for here work and break what is written everywhere else.
fn build_response(context: &mut Context, url: &str, response: Response) -> u64 {
    let object = entry::make_object(context);
    entry::put_member(context, object, "status", entry::make_number(response.status as f64));
    let reason = entry::make_string(context, &response.reason);
    entry::put_member(context, object, "statusText", reason);
    let ok = (200..300).contains(&response.status);
    entry::put_member(context, object, "ok", entry::boolean_value(ok));
    entry::put_member(context, object, "redirected", entry::boolean_value(false));
    let address = entry::make_string(context, url);
    entry::put_member(context, object, "url", address);
    let type_ = entry::make_string(context, "basic");
    entry::put_member(context, object, "type", type_);

    // The headers as a plain object, with lowercased names — the shape a
    // `Headers` answers and what a program compares against. Not the
    // `Headers` class: `get`/`has`/`forEach` are not here, and inventing
    // them half-done would be worse than the absence.
    let headers = entry::make_object(context);
    for (name, value) in &response.headers {
        let value = entry::make_string(context, value);
        entry::put_member(context, headers, &name.to_lowercase(), value);
    }
    entry::put_member(context, object, "headers", headers);

    // The body is kept as both text and bytes, and the methods read from
    // here. `__body__` because the name is not meant to be read by whoever
    // uses this.
    let text = String::from_utf8_lossy(&response.body).into_owned();
    let stored = entry::make_string(context, &text);
    entry::put_member(context, object, "__body__", stored);
    entry::put_member(context, object, "bodyUsed", entry::boolean_value(false));

    let text_fn = entry::make_callable(context, body_text);
    entry::put_member(context, object, "text", text_fn);
    let json_fn = entry::make_callable(context, body_json);
    entry::put_member(context, object, "json", json_fn);
    object
}

/// `res.text()` — a promise of the body, as in a browser.
extern "C" fn body_text(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let body = entry::get_member(context, this, "__body__");
        entry::put_member(context, this, "bodyUsed", entry::boolean_value(true));
        entry::settled(context, body, false)
    })
}

/// `res.json()` — the same, passed through `JSON.parse`.
///
/// A body that is not JSON REJECTS, which is what a browser does. Answering
/// `undefined` would leave the program summing from nothing.
extern "C" fn body_json(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let body = entry::with_runtime(|context| entry::get_member(context, this, "__body__"));
    let text = entry::text_of(body).unwrap_or_default();
    // Through the PROGRAM's own `JSON.parse`, not a parser of this module's:
    // a second JSON reader would be a second answer to the same question,
    // and the two would come to disagree on some corner of the grammar.
    let parse_and_text = entry::with_runtime(|context| {
        let global = entry::global_object(context);
        let json = entry::get_member(context, global, "JSON");
        let parse = entry::get_member(context, json, "parse");
        (parse, entry::make_string(context, &text))
    });
    let absent = entry::undefined_value();
    let parsed = entry::call(parse_and_text.0, absent, parse_and_text.1, absent, absent, absent);
    if entry::pending().is_some() {
        entry::take_thrown();
        return reject("res.json(): the body is not JSON");
    }
    entry::with_runtime(|context| {
        entry::put_member(context, this, "bodyUsed", entry::boolean_value(true));
        entry::settled(context, parsed, false)
    })
}

/// `__rtsFetchText(url)` — a GET's body, as text, WITHOUT a promise.
///
/// Exists for a caller that cannot await: `rts-dom`'s `__readResource`, which
/// loads a stylesheet or a script off `http(s)://` in the middle of
/// assembling a document. It used to call `fetch.fetchText(...)` on an
/// `rts:fetch` namespace that DIED with the old engine — its comment still
/// describes that namespace, and the call answered nothing.
///
/// `""` on error, the tolerant convention that caller already uses: a
/// resource that fails to load leaves the page without it, not without a
/// page.
///
/// The `__` name says what it is: a door for the prelude, not surface for a
/// program. Code that gets written uses `fetch`.
pub(crate) extern "C" fn fetch_text(_e: u64, _t: u64, url: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let Some(text) = entry::text_of(url) else {
        return entry::with_runtime(|context| entry::make_string(context, ""));
    };
    let body = match request(&text, "GET", "", &[]) {
        Ok(response) => String::from_utf8_lossy(&response.body).into_owned(),
        Err(_) => String::new(),
    };
    entry::with_runtime(|context| entry::make_string(context, &body))
}

#[cfg(test)]
mod tests {
    // Pure-function coverage of the "reason instead of None" rewrite, and of
    // the chunked-completion fix, lives in `request::tests` — the functions
    // that would need an end-to-end test (`open_socket`, `wait_connected`,
    // `read_response`) all call into a live `rts_core::Context` through
    // `entry::with_runtime`/`entry::call` to reach a `node:net`/`node:tls`
    // socket, and this crate has no test-side bootstrap that builds one
    // (every other test of this surface runs as a `*.test.ts` fixture under
    // `rts-host`, a full compiled program). Adding that bootstrap here would
    // be a second one, not a reuse of the first.
}
