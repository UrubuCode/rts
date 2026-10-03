//! How a `ClientRequest` reads its response: by LISTENING, not by blocking.
//!
//! # The deadlock this removes, and how it was measured
//!
//! `client.rs` used to read the response with `read_response_blocking` — a loop
//! on the JavaScript thread that called `socket.write(empty)` to force
//! `net::registry::pump` and then slept 4 ms. Over loopback, with the server in
//! the SAME program, that cannot complete, and the reason is not the sleep:
//!
//! `net::registry::pump` carries a thread-local reentrancy guard (`ENTREGANDO`),
//! added so a listener that writes from inside a `'data'` delivery cannot drain
//! the queue the outer pump is still walking. A same-program exchange starts
//! inside a pump — `server.listen(..., callback)` runs the program's callback
//! from `pump_servers`' `'listening'` delivery — so every nested pump the client
//! loop asked for returned IMMEDIATELY without delivering anything. The server's
//! `'connection'` was sitting in the queue the outer pump had already passed, and
//! the only thing that could deliver it was the loop the client was blocking.
//! Measured before: `listening` then `req error ETIMEDOUT` after ten seconds.
//! Node 22, same program: `listening, client:socket, server:request GET /hi,
//! server:req-end, client:response 200, client:data, client:end body=pong`.
//!
//! Spinning harder would not have helped and a bigger reentrancy budget would
//! have reintroduced the out-of-order `'data'` that guard exists to stop. What
//! was wrong is that the client read at all: nothing else in this crate does.
//!
//! # The form, and it is not a new one
//!
//! `super::server` already reads a request off a `net.Socket` by attaching
//! `'data'`/`'end'` and running the pure `parser.rs` state machine over a
//! per-connection buffer held in a Rust table, turning JS-visible consequences
//! into a list of effects performed after the table lock is dropped. This module
//! is the same thing with `parse_response_head` in place of
//! `parse_request_head` — the client half `super::registry`'s doc said did not
//! exist because the client blocked.
//!
//! So: no new loop source and no new polling. `net`'s own source already
//! delivers `'data'` to a program that only waits (an open socket answers
//! `Pending::In`), and this module is one of its listeners. Rejected: a loop
//! source of its own that drains the socket on every pass — it would be a second
//! answer to "how does pending work reach the loop" for a socket that already
//! has one, and it would add a second wake-up rate on top of `net`'s 1 ms.
//!
//! # Two contracts this changes on purpose
//!
//! `request()` and `end()` now RETURN before the response arrives, which is
//! Node's contract and the opposite of what `client.rs`' doc used to promise.
//! `'response'` is emitted from a later pump, so `req.on('response', …)` written
//! after `end()` is reached in time — it never was before.
//!
//! And a response with neither `Content-Length` nor `Transfer-Encoding: chunked`
//! is now terminated by EOF, which is what HTTP/1.1 says for a response (never
//! for a request, which is why `server.rs` keeps treating that framing as "no
//! body"). It used to answer an empty body, so every `Connection: close` server
//! that simply wrote and closed read as a 0-byte reply here.

use rts_core::entry;

use super::common::*;
use super::registry::{self, ClientConn, ClientStage};
use super::{incoming, parser};

/// Starts reading `socket` as `request`'s response.
///
/// Called by `client_end` right after the request bytes go out, and never
/// before: a `ClientRequest` this crate builds sends exactly one framed request
/// (`client.rs`), so there is no pipelining state to keep.
pub(super) fn begin(request: u64, socket: u64) {
    let id = assign_id(socket);
    registry::with_client_conns(|table| {
        table.insert(id, ClientConn { request, socket, buf: Vec::new(), stage: ClientStage::Head });
    });
    let absent = entry::undefined_value();
    let data_fn = entry::with_runtime(|context| entry::make_callable(context, on_data));
    let end_fn = entry::with_runtime(|context| entry::make_callable(context, on_end));
    call_method(socket, "on", key("data"), data_fn, absent);
    call_method(socket, "on", key("end"), end_fn, absent);
    // `resume()` and not merely the two listeners above: `stream`'s own doc says
    // attaching `'data'` does not switch a Duplex into flowing mode here, and a
    // paused socket buffers the response forever. Same call `server::on_connection`
    // makes on an accepted socket, for the same reason.
    call_method(socket, "resume", absent, absent, absent);
}

/// Forgets `socket`'s read state — called when the request is destroyed, so an
/// aborted exchange does not leave an entry nothing will ever complete.
pub(super) fn forget(socket: u64) {
    let Some(id) = read_id(socket) else { return };
    registry::with_client_conns(|table| {
        table.remove(&id);
    });
}

/// This module's own identity for the socket it is reading.
///
/// NOT `net`'s `__socketId`, which is what `super::server` keys its accepted
/// connections by. A `https.request` runs `http`'s own `client_end` on a
/// `tls.TLSSocket` (that module's doc states the sharing), and a `TLSSocket`
/// carries no `__socketId` — only the plain socket it wraps does. Keying by it
/// would therefore have silently read nothing for every HTTPS request, which is
/// the shape of defect this crate keeps paying for: an identity that exists for
/// one caller and not the other.
const ID: &str = "__httpReadId__";

static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn assign_id(socket: u64) -> u64 {
    if let Some(held) = read_id(socket) {
        return held;
    }
    let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    entry::with_runtime(|context| set_num(context, socket, ID, id as f64));
    id
}

fn read_id(socket: u64) -> Option<u64> {
    match entry::number_of(get_value(socket, ID)) {
        Some(n) if n >= 1.0 => Some(n as u64),
        _ => None,
    }
}

extern "C" fn on_data(_e: u64, socket: u64, chunk: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(id) = read_id(socket) else { return entry::undefined_value() };
    let bytes = entry::with_runtime(|context| entry::bytes_of(context, chunk)).unwrap_or_default();
    registry::with_client_conns(|table| {
        if let Some(conn) = table.get_mut(&id) {
            conn.buf.extend_from_slice(&bytes);
        }
    });
    drain(id, false);
    entry::undefined_value()
}

/// The peer closed its write half: whatever is still buffered is all there is.
///
/// This is where an EOF-framed body ends, which is why `eof` is a parameter of
/// [`drain`] rather than a second code path — a `Framing::None` response is
/// complete exactly here and nowhere else.
extern "C" fn on_end(_e: u64, socket: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(id) = read_id(socket) else { return entry::undefined_value() };
    drain(id, true);
    registry::with_client_conns(|table| {
        table.remove(&id);
    });
    entry::undefined_value()
}

enum Effect {
    Response { request: u64, message: u64 },
    Body { message: u64, bytes: Vec<u8> },
    End { message: u64 },
}

/// Runs [`advance`] with the connection OUT of the table, then performs the
/// effects — the same lock-then-collect-then-call discipline
/// `server::drain` and `net::registry::pump` document: every effect below calls
/// a listener the program wrote, and that listener calls back into this module
/// (`req.destroy()` from inside `'response'` is the ordinary case), onto a
/// `std::sync::Mutex` that is not reentrant.
fn drain(id: u64, eof: bool) {
    let Some(mut conn) = registry::with_client_conns(|table| table.remove(&id)) else { return };
    let mut effects = Vec::new();
    advance(&mut conn, eof, &mut effects);
    let done = matches!(conn.stage, ClientStage::Done);
    if !done {
        registry::with_client_conns(|table| {
            table.insert(id, conn);
        });
    }
    let absent = entry::undefined_value();
    for effect in effects {
        match effect {
            Effect::Response { request, message } => {
                emit(request, "response", message, absent, absent);
            }
            Effect::Body { message, bytes } => {
                let push_fn = entry::with_runtime(|context| entry::get_member(context, message, "push"));
                // BUFFER and not `Uint8Array`, the same choice `net::registry`'s
                // `'data'` delivery documents: `chunk.toString()` on a
                // Uint8Array answers the bytes as a comma-separated list where a
                // program expects the decoded text.
                let chunk = entry::with_runtime(|context| entry::make_buffer(context, &bytes));
                entry::call(push_fn, message, chunk, absent, absent, absent);
            }
            Effect::End { message } => {
                let push_fn = entry::with_runtime(|context| entry::get_member(context, message, "push"));
                let null = entry::null_value();
                entry::call(push_fn, message, null, absent, absent, absent);
                entry::with_runtime(|context| set_bool(context, message, "complete", true));
            }
        }
    }
}

/// The pure-parsing step: consumes as much of `conn.buf` as is decidable now,
/// appending every JS-visible consequence to `effects` rather than acting.
///
/// `entry::with_runtime` appears here only to BUILD the `IncomingMessage`, which
/// calls no user code — emitting is the caller's job.
fn advance(conn: &mut ClientConn, eof: bool, effects: &mut Vec<Effect>) {
    loop {
        match &mut conn.stage {
            ClientStage::Head => {
                let Some((head, consumed)) = parser::parse_response_head(&conn.buf) else { return };
                conn.buf.drain(..consumed);
                let socket = conn.socket;
                let request = conn.request;
                let message = entry::with_runtime(|context| {
                    let message =
                        incoming::build_incoming(context, socket, &head.headers, &head.version, None, Some((head.status, head.reason.as_str())));
                    // Hung off the request as well as held in this table: rule 10
                    // of `rts-core`'s README — a reference a Rust table holds and
                    // nothing else does is a reference the collector is not told
                    // about, and this table is not one of `side_tables`' variants.
                    // A property of the request, which the program itself holds,
                    // is a root that already exists.
                    set_value(context, request, "__response__", message);
                    message
                });
                let framing = match parser::framing_of(&head.headers) {
                    parser::Framing::Chunked => registry::Framing::Chunked(parser::ChunkedDecoder::new()),
                    parser::Framing::Length(n) => registry::Framing::Length(n),
                    // A 1xx/204/304 answers no body whatever the headers say, and
                    // so does a response to HEAD. Everything else with no length
                    // and no chunking runs to EOF — see the module doc for why
                    // this differs from the request side's reading of the same
                    // `Framing::None`.
                    parser::Framing::None => match bodyless(head.status, request) {
                        true => registry::Framing::Length(0),
                        false => registry::Framing::None,
                    },
                };
                effects.push(Effect::Response { request, message });
                conn.stage = ClientStage::Body { message, framing };
            }
            ClientStage::Body { message, framing } => match framing {
                // EOF-framed: everything buffered is body, and the message only
                // ends when the peer's write half does.
                registry::Framing::None => {
                    let msg = *message;
                    if !conn.buf.is_empty() {
                        let bytes: Vec<u8> = conn.buf.drain(..).collect();
                        effects.push(Effect::Body { message: msg, bytes });
                    }
                    if eof {
                        effects.push(Effect::End { message: msg });
                        conn.stage = ClientStage::Done;
                    }
                    return;
                }
                registry::Framing::Length(remaining) => {
                    let msg = *message;
                    if *remaining > 0 && !conn.buf.is_empty() {
                        let take = (*remaining).min(conn.buf.len());
                        let bytes: Vec<u8> = conn.buf.drain(..take).collect();
                        *remaining -= take;
                        effects.push(Effect::Body { message: msg, bytes });
                    }
                    // A peer that closed before owing bytes arrived ends the
                    // message anyway: reporting a truncation needs an error
                    // channel on `IncomingMessage` this crate does not have, and
                    // never ending leaks the entry and hangs the program.
                    if *remaining == 0 || eof {
                        effects.push(Effect::End { message: msg });
                        conn.stage = ClientStage::Done;
                    }
                    return;
                }
                registry::Framing::Chunked(decoder) => loop {
                    match decoder.step(&mut conn.buf) {
                        parser::ChunkOutcome::NeedMore => {
                            if eof {
                                effects.push(Effect::End { message: *message });
                                conn.stage = ClientStage::Done;
                            }
                            return;
                        }
                        parser::ChunkOutcome::Body(bytes) => effects.push(Effect::Body { message: *message, bytes }),
                        parser::ChunkOutcome::Done => {
                            effects.push(Effect::End { message: *message });
                            conn.stage = ClientStage::Done;
                            return;
                        }
                    }
                },
            },
            ClientStage::Done => return,
        }
    }
}

/// Whether this status, for this request, carries no body at all.
///
/// The method is read off the request rather than remembered here: the
/// `ClientRequest` already holds it as a public property, and a second copy in
/// this table is a second thing that can disagree.
fn bodyless(status: u16, request: u64) -> bool {
    if (100..200).contains(&status) || status == 204 || status == 304 {
        return true;
    }
    get_text(request, "method").is_some_and(|method| method.eq_ignore_ascii_case("HEAD"))
}
