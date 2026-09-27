//! The wire: turns a URL, a method and a body into bytes on a socket and
//! bytes back, with a REASON attached to every way that can fail.
//!
//! Split out of `fetch.rs` once the reason-carrying rewrite pushed the file
//! over this workspace's 500-line ceiling — this half is the socket/parse
//! plumbing, `mod.rs` keeps the `Response` object and the two entry points.
//!
//! # Every `None` this module used to answer, now a `String`
//!
//! Before this change, `request` returned `Option<Response>` and `fetch()`
//! turned a `None` into a bare `TypeError: fetch: <url> falhou` — no matter
//! which of [`split_url`] (bad URL), [`open_socket`] (connect),
//! [`wait_connected`] (TLS handshake) or [`read_response`] (the read loop)
//! gave up, nor after how many bytes. That is workable in isolation and
//! useless on an intermittent failure: a multi-MB bundle over TLS that fails
//! 2 runs out of 3 gives no way to tell "the handshake never finished" from
//! "the body stopped 400KB short" from "the socket errored after the promise
//! had nothing left to check". Every function below now returns
//! `Result<_, String>`, and the reason is a sentence a person reads, not a
//! variant they look up.
//!
//! # Where the reason for a socket failure comes from
//!
//! `open_socket`/`wait_connected`/`read_response` don't get a `Result` back
//! from `net`/`tls` — a `node:net` socket reports a failed connect or a
//! broken TLS handshake by EMITTING `'error'` on the socket, asynchronously,
//! the same way Node's own does. The old code installed a listener for
//! exactly that reason (an unheard `'error'` event kills the process), but it
//! was a no-op — the message was thrown away at the one place it was
//! available. [`capture_error`] is that same listener with the text kept, in
//! the `LAST_ERROR` `thread_local` (the same shape `net::registry` already
//! uses for its own per-thread state), and [`take_error`] is how the loops
//! below pick it up.
//!
//! # Why a chunked body no longer waits for the socket to close
//!
//! A body with `Transfer-Encoding: chunked` used to be treated exactly like a
//! body with no framing at all: complete only once the socket closed. That
//! is correct but slow — a server or a CDN in front of it is not obliged to
//! close the connection promptly after the last chunk, `Connection: close`
//! notwithstanding — and it is a plausible cause of an intermittent timeout
//! on a large chunked body over TLS: the bytes are all there, but nothing
//! checks for the chunk stream's own terminator, so the read waits out the
//! full [`super::TIMEOUT_MS`] regardless. [`chunked_body_complete`] runs the
//! SAME decoder `crate::http::client::decode_body` uses
//! (`crate::http::parser::ChunkedDecoder`, widened from `pub(super)` to
//! `pub(crate)` for this reuse) over a scratch copy, instead of
//! reimplementing the chunk grammar a second time — which is the duplication
//! this repository's `reuse-check` exists to catch. `read_response` now
//! completes on `chunked_body_complete(&remaining) || closed(socket)`: either
//! is enough, and the socket-close check stays as the fallback for a
//! malformed trailer the decoder never reports `Done` for — the timeout still
//! bounds that case either way.

use std::time::{Duration, Instant};

use rts_core::entry;

use super::{Response, TIMEOUT_MS};

/// The `User-Agent` this engine claims to be.
///
/// The SAME string as `window.ts`'s — a real, assumed duplication: the two
/// crates don't see each other (`rts-node` does not depend on `rts-dom` nor
/// the reverse), and a third place just for this would be a third thing to
/// keep in sync instead of two. Whoever changes one changes the other.
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

/// Makes the whole request and returns what came back, or a reason it did not.
pub(super) fn request(url: &str, method: &str, body: &str, extra_headers: &[(String, String)]) -> Result<Response, String> {
    let (secure, host, port, path) = split_url(url)?;
    let socket = open_socket(&host, port, secure)
        .map_err(|reason| format!("connect to {host}:{port} failed: {reason}"))?;
    wait_connected(socket, secure).map_err(|reason| match secure {
        true => format!("TLS handshake with {host} failed: {reason}"),
        false => format!("connect to {host}:{port} failed: {reason}"),
    })?;

    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\n");
    head.push_str("Connection: close\r\n");
    // The PROGRAM's headers first, ours only where it said nothing: a
    // `fetch` that ignored a chosen `User-Agent` would be a `fetch` that
    // decides for the program.
    let declared = |name: &str| extra_headers.iter().any(|(n, _)| n.eq_ignore_ascii_case(name));
    for (name, value) in extra_headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    if !declared("user-agent") {
        head.push_str(&format!("User-Agent: {USER_AGENT}\r\n"));
    }
    if !declared("accept") {
        head.push_str("Accept: */*\r\n");
    }
    if !body.is_empty() && !declared("content-length") {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("\r\n");
    head.push_str(body);
    write_all(socket, head.as_bytes());

    read_response(socket)
}

/// `https://host:port/path` in its four parts.
fn split_url(url: &str) -> Result<(bool, String, u16, String), String> {
    let (secure, rest) = match url.strip_prefix("https://") {
        Some(rest) => (true, rest),
        None => match url.strip_prefix("http://") {
            Some(rest) => (false, rest),
            None => return Err(format!("unsupported URL scheme (expected http:// or https://): {url}")),
        },
    };
    let (authority, path) = match rest.find('/') {
        Some(at) => (&rest[..at], &rest[at..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (
            h.to_owned(),
            p.parse().map_err(|_| format!("invalid port in URL: {url}"))?,
        ),
        None => (authority.to_owned(), if secure { 443 } else { 80 }),
    };
    Ok((secure, host, port, path.to_owned()))
}

// The message of the last `'error'` event a socket this module opened
// emitted, captured by `capture_error` — installed instead of the old
// silent listener — so `open_socket`/`wait_connected`/`read_response` have a
// REASON instead of just knowing something went wrong. A `thread_local`
// because this engine's `Context` already is one (the same pattern
// `net::registry` uses).
thread_local! {
    static LAST_ERROR: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// The `'error'` listener for a socket this module opens, with the text KEPT
/// instead of discarded. `entry::get_member` stays inside the borrow, the
/// `entry::text_of` of the value it returns stays outside — the same rule
/// this file already explains twice.
extern "C" fn capture_error(_e: u64, _t: u64, error: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let message = entry::with_runtime(|context| entry::get_member(context, error, "message"));
    let text = entry::text_of(message);
    LAST_ERROR.with(|c| *c.borrow_mut() = text);
    entry::undefined_value()
}

/// The reason [`capture_error`] captured, if any arrived since the last time
/// this was called — takes it, so a second read does not repeat an error
/// from an earlier connection.
fn take_error() -> Option<String> {
    LAST_ERROR.with(|c| c.borrow_mut().take())
}

/// A socket connected to the host, with TLS when the scheme asks for it.
fn open_socket(host: &str, port: u16, secure: bool) -> Result<u64, String> {
    LAST_ERROR.with(|c| *c.borrow_mut() = None);
    let absent = entry::undefined_value();
    let ns = entry::with_runtime(|context| match secure {
        true => crate::tls::namespace(context),
        false => crate::net::namespace(context),
    });
    let connect_fn = entry::with_runtime(|context| entry::get_member(context, ns, "connect"));
    let options = entry::with_runtime(|context| {
        let object = entry::make_object(context);
        // The SNI. Without it a modern server does not know WHICH
        // certificate to serve and refuses the connection — this is what set
        // this path apart from `node:https`, which has always passed it.
        let name = entry::make_string(context, host);
        entry::put_member(context, object, "servername", name);
        let host = entry::make_string(context, host);
        entry::put_member(context, object, "host", host);
        entry::put_member(context, object, "port", entry::make_number(f64::from(port)));
        // A certificate that does not validate is not a reason to not READ
        // the page, and refusing here would give a `fetch` that fails on
        // half the internet over a policy nobody asked for. Said, not
        // hidden.
        entry::put_member(context, object, "rejectUnauthorized", entry::boolean_value(false));
        object
    });
    let socket = entry::call(connect_fn, absent, options, absent, absent, absent);
    // An `on("error")` BEFORE anything else. A socket that fails to connect —
    // TLS that never negotiates, a host that does not exist — emits `error`,
    // and an `error` nobody listened to KILLS the program: `uncaught 'error'
    // event`. The failure already has an answer here (the promise rejects),
    // and now it also has a REASON: `capture_error` keeps the message
    // instead of discarding it, and `open_socket`/`wait_connected`/
    // `read_response` read it from `LAST_ERROR`.
    let listener = entry::with_runtime(|context| entry::make_callable(context, capture_error));
    let name = entry::with_runtime(|context| entry::make_string(context, "error"));
    let on = entry::with_runtime(|context| entry::get_member(context, socket, "on"));
    entry::call(on, socket, name, listener, absent, absent);
    if socket == absent {
        return Err(take_error().unwrap_or_else(|| "connect() returned nothing".to_owned()));
    }
    Ok(socket)
}

/// Waits for the socket to connect. `Err` with the reason if it gave up or
/// the socket emitted `'error'` in the meantime.
fn wait_connected(socket: u64, secure: bool) -> Result<(), String> {
    let started_at = Instant::now();
    loop {
        let empty = entry::with_runtime(|context| entry::make_bytes(context, &[]));
        // The `write` below is what PUMPS the queued events (`registry::pump`,
        // in this crate's `net`/`tls`) — that is how an `'error'` that already
        // arrived becomes visible to `take_error` before looking at
        // `connecting`/`getProtocol` again.
        call(socket, "write", empty);
        if let Some(reason) = take_error() {
            return Err(reason);
        }
        // A TLS socket says it connected once the HANDSHAKE ends, and that
        // reads from `getProtocol()` — a plain socket's `connecting` is
        // already false before that, and writing there sends bytes down a
        // tunnel that does not exist yet.
        if secure {
            let protocol = call(socket, "getProtocol", entry::undefined_value());
            if entry::text_of(protocol).is_some_and(|p| p != "undefined" && !p.is_empty()) {
                return Ok(());
            }
        } else {
            let connecting = entry::with_runtime(|context| entry::get_member(context, socket, "connecting"));
            if connecting != entry::boolean_value(true) {
                return Ok(());
            }
        }
        if started_at.elapsed() > Duration::from_millis(TIMEOUT_MS) {
            return Err(format!("timed out after {}s", TIMEOUT_MS / 1000));
        }
        std::thread::sleep(Duration::from_millis(4));
    }
}

fn write_all(socket: u64, bytes: &[u8]) {
    let data = entry::with_runtime(|context| entry::make_bytes(context, bytes));
    call(socket, "write", data);
}

/// Whether a `Transfer-Encoding: chunked` body received so far is COMPLETE —
/// its terminating zero-length chunk (and any trailers) have arrived.
///
/// Runs the same [`crate::http::parser::ChunkedDecoder`]
/// `crate::http::client::decode_body` uses, over a scratch copy, rather than
/// a second reader of the chunk grammar — see this module's doc for why this
/// replaces waiting on the socket to close.
fn chunked_body_complete(buf: &[u8]) -> bool {
    let mut scratch = buf.to_vec();
    let mut decoder = crate::http::parser::ChunkedDecoder::new();
    loop {
        match decoder.step(&mut scratch) {
            crate::http::parser::ChunkOutcome::Body(_) => continue,
            crate::http::parser::ChunkOutcome::Done => return true,
            crate::http::parser::ChunkOutcome::NeedMore => return false,
        }
    }
}

/// Reads until the response is complete. `Err` with the reason — and how many
/// bytes had already arrived — if time ran out or the socket emitted
/// `'error'` before that.
fn read_response(socket: u64) -> Result<Response, String> {
    let started_at = Instant::now();
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.extend_from_slice(&drain(socket));
        if let Some(reason) = take_error() {
            return Err(format!("read: {reason} (after {} bytes)", buf.len()));
        }
        if let Some((head, consumed)) = crate::http::parser::parse_response_head(&buf) {
            let framing = crate::http::parser::framing_of(&head.headers);
            let mut remaining = buf[consumed..].to_vec();
            let target = match framing {
                crate::http::parser::Framing::Length(n) => Some(n),
                crate::http::parser::Framing::None | crate::http::parser::Framing::Chunked => None,
            };
            loop {
                let enough = match (target, framing) {
                    (Some(n), _) => remaining.len() >= n,
                    // A chunked body is complete once its OWN terminator has
                    // arrived, not only once the socket closes — see
                    // `chunked_body_complete`. `closed(socket)` stays as a
                    // safety net: if the decoder never reports `Done`
                    // (malformed trailers, say), the close still resolves it,
                    // and `TIMEOUT_MS` bounds both cases regardless.
                    (None, crate::http::parser::Framing::Chunked) => {
                        chunked_body_complete(&remaining) || closed(socket)
                    }
                    // With neither `Content-Length` nor chunking, the end is
                    // the socket closing — which the request's
                    // `Connection: close` guarantees.
                    (None, _) => closed(socket),
                };
                if enough {
                    break;
                }
                if started_at.elapsed() > Duration::from_millis(TIMEOUT_MS) {
                    let reason = match (target, framing) {
                        (Some(n), _) => format!(
                            "read timed out after {}s: {} of {n} body bytes",
                            TIMEOUT_MS / 1000,
                            remaining.len()
                        ),
                        (None, crate::http::parser::Framing::Chunked) => format!(
                            "read timed out after {}s: chunked body never reached its terminating chunk ({} bytes buffered)",
                            TIMEOUT_MS / 1000,
                            remaining.len()
                        ),
                        (None, _) => format!(
                            "read timed out after {}s waiting for the connection to close ({} body bytes)",
                            TIMEOUT_MS / 1000,
                            remaining.len()
                        ),
                    };
                    return Err(reason);
                }
                remaining.extend_from_slice(&drain(socket));
                if let Some(reason) = take_error() {
                    return Err(format!("read: {reason} (after {} body bytes)", remaining.len()));
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            // De-chunked by the same `decode_body` `node:http` uses: a
            // `Transfer-Encoding: chunked` body arrives with each piece's
            // size in hex ahead of it, and handing that over as-is gave a
            // `res.text()` starting with `22f` — what happened on this
            // module's first successful request.
            return Ok(Response {
                status: i64::from(head.status),
                reason: head.reason.clone(),
                headers: head.headers.clone(),
                body: crate::http::client::decode_body(&remaining, framing),
            });
        }
        if started_at.elapsed() > Duration::from_millis(TIMEOUT_MS) {
            return Err(format!(
                "read timed out after {}s waiting for the response headers ({} bytes received)",
                TIMEOUT_MS / 1000,
                buf.len()
            ));
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn closed(socket: u64) -> bool {
    entry::with_runtime(|context| entry::get_member(context, socket, "destroyed"))
        == entry::boolean_value(true)
}

/// Takes everything the socket has right now.
///
/// The empty write first, then `read` in a LOOP until it gives `null` — the
/// same thing `http::client::drain_socket_buffer` does, and its own module
/// has why: with no loop iteration happening on its own, it is the write that
/// forces the socket to progress. A single read handed back the first piece
/// and lost the rest.
fn drain(socket: u64) -> Vec<u8> {
    let absent = entry::undefined_value();
    let empty = entry::with_runtime(|context| entry::make_bytes(context, &[]));
    call(socket, "write", empty);
    let mut out = Vec::new();
    loop {
        let chunk = call(socket, "read", absent);
        if chunk == entry::null_value() || chunk == absent {
            return out;
        }
        if let Some(bytes) = entry::with_runtime(|context| entry::bytes_of(context, chunk)) {
            out.extend_from_slice(&bytes);
        }
    }
}

fn call(object: u64, name: &str, argument: u64) -> u64 {
    let absent = entry::undefined_value();
    let method = entry::with_runtime(|context| entry::get_member(context, object, name));
    entry::call(method, object, argument, absent, absent, absent)
}

#[cfg(test)]
mod tests {
    use super::{chunked_body_complete, split_url};

    // `split_url` and `chunked_body_complete` are the two functions here that
    // need no `Context` — pure bytes/text in, an answer or a reason out — so
    // they are the part of this rewrite a plain unit test can pin without a
    // socket or a JS runtime.

    #[test]
    fn rejects_a_url_with_no_recognised_scheme() {
        let error = split_url("ftp://example.com/file").unwrap_err();
        assert!(error.contains("http://") && error.contains("https://"), "{error}");
    }

    #[test]
    fn rejects_a_port_that_does_not_parse() {
        let error = split_url("http://example.com:not-a-port/x").unwrap_err();
        assert!(error.contains("invalid port"), "{error}");
    }

    #[test]
    fn defaults_the_port_by_scheme() {
        assert_eq!(split_url("http://example.com/x").unwrap().2, 80);
        assert_eq!(split_url("https://example.com/x").unwrap().2, 443);
    }

    #[test]
    fn splits_host_port_and_path() {
        let (secure, host, port, path) = split_url("https://example.com:8443/a/b?c=1").unwrap();
        assert!(secure);
        assert_eq!(host, "example.com");
        assert_eq!(port, 8443);
        assert_eq!(path, "/a/b?c=1");
    }

    #[test]
    fn defaults_the_path_to_root() {
        assert_eq!(split_url("http://example.com").unwrap().3, "/");
    }

    #[test]
    fn a_buffer_ending_in_the_zero_chunk_is_complete() {
        assert!(chunked_body_complete(b"5\r\nhello\r\n0\r\n\r\n"));
    }

    #[test]
    fn a_buffer_missing_the_zero_chunk_is_not_complete() {
        // A full first chunk, no terminator yet — the shape a large chunked
        // body has for most of its transfer, and the exact case that used to
        // wait out the whole timeout instead of noticing it already has
        // everything once the terminator DOES arrive later in the same
        // buffer.
        assert!(!chunked_body_complete(b"5\r\nhello\r\n"));
    }

    #[test]
    fn an_empty_buffer_is_not_complete() {
        assert!(!chunked_body_complete(b""));
    }
}
