// node:http2 — THE killer call: a server receiving ANY real request panics
// the whole process. This is the single most important finding in this
// batch — it means a `node:http2` server built from this module can never
// successfully answer one single request, from ANY client, ever.
//
// ============================================================================
// Repro, standalone, via `target/fast/rts.exe run` on a throwaway script:
// a real http2.createServer() bound and listening, a real http2.connect()
// client, `session.request({...})` called once, then a `setTimeout` to let
// the event loop actually deliver the queued events (the crash does NOT
// happen synchronously inside `session.request()` itself — sending HEADERS
// only writes bytes to the socket; the crash happens LATER, the first time
// the host's event loop pumps the SERVER's queue and tries to build the
// JS object for the newly-arrived, peer-opened stream).
// ============================================================================
//
//   [RTS PANIC] make_prototype("Http2Stream") collision: already owned by
//   crates\rts-node\src\http2\js.rs, also claimed by
//   crates\rts-node\src\http2\delivery.rs — two modules registered
//   different method tables under one prototype name; rename one (e.g.
//   "module.Http2Stream") or, if the sharing is deliberate and
//   self-healing, add it to SHARED_BY_DESIGN with the same reasoning as
//   its existing entries
//     at crates\rts-node\src\http2\delivery.rs:100:29
//
//   Panic backtrace (the frames that carry information):
//     rts_core::entry::modules::make_prototype        <- panics HERE
//     rts_node::http2::delivery::deliver               (folded/mangled in
//                                                        the raw trace, but
//                                                        this is where
//                                                        make_prototype is
//                                                        called from — see
//                                                        below)
//     rts_node::http2::delivery::pump
//     rts_node::http2::js::session_prototype  <-  called from...
//     rts_core::entry::loops::pump_sources     <-  the HOST's own event
//                                                   loop, servicing
//                                                   node:http2 as a
//                                                   registered loop source
//
// ROOT CAUSE, read directly (no guessing — this is a real engine
// self-check, not a heap corruption): `js.rs` declares its own
// `STREAM_METHODS` constant and registers it under the prototype name
// `"Http2Stream"` (used when a CLIENT builds the stream `session.request()`
// returns). `delivery.rs` declares a SECOND, separately-defined
// `STREAM_METHODS` constant of its own — same four (name, fn) pairs,
// pointing at the exact same four functions
// (`js::stream_respond`/`stream_write`/`stream_end`/`stream_close`) — and
// registers IT under the SAME name `"Http2Stream"`, the moment a server
// needs to build the JS object for a stream a PEER opened (i.e. the moment
// any real client's request headers arrive). The engine's prototype
// registry tracks a name's owner by SOURCE LOCATION, not by content
// equality, so two textually-identical-but-separately-declared method
// tables under one name trip the collision guard every single time,
// unconditionally — this is not a race, not a timing issue, not
// intermittent.
//
// Practical consequence: `http2.createServer()` can accept a TCP connection
// and a full HTTP/2 SETTINGS exchange (both proven safe and real in
// `claude-node-http2.test.ts`) — it just can never process a single
// request. The very first HEADERS frame from ANY client crashes the
// process delivering the 'stream' event that request would have produced.
//
// THE KILLER CALL (left unrun — uncomment the block below to reproduce; a
// setTimeout is REQUIRED for the crash to surface, since it happens during
// event-loop delivery, not inside session.request() itself):
//
// import * as http2 from "node:http2";
// const server: any = http2.createServer();
// server.on("session", (session: any) => {
//   session.on("stream", (stream: any, headers: any) => {
//     stream.respond({ ":status": "200" });
//     stream.end("hello");
//   });
// });
// server.listen(0);
// const port = server.port;
// const client: any = http2.connect("http://127.0.0.1:" + port);
// client.request({
//   ":method": "GET", ":scheme": "http",
//   ":authority": "127.0.0.1:" + port, ":path": "/",
// });
// setTimeout(() => {
//   // never reached — the process panics during this timer's own delivery
//   // pass, before this callback body runs.
// }, 200);

import { describe, test, expect } from "rts:test";

// The only thing this file safely measures: importing the module, and a
// session ACCEPT with no request sent (proven safe in the companion file),
// does not by itself trigger the crash — only a delivered request does.
import * as http2 from "node:http2";
const importedSafelyOk = typeof http2.createServer === "function";

describe("node:http2 — importing the module alone is safe (the crash needs a real request)", () => {
    test("node:http2 imports without crashing", () => expect(importedSafelyOk).toBe(true));
});
