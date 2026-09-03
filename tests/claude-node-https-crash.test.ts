// node:https — TWO killer calls, isolated. Neither is run — see each
// section for the exact repro and why it is left commented out.
//
// ============================================================================
// KILLER CALL #1 — https.request() against a REAL, listening https server
// panics the WHOLE PROCESS with a nested-borrow abort, before a single
// request byte is even sent.
// ============================================================================
//
// Reproduced standalone via `target/fast/rts.exe run` on a throwaway script
// (a real self-signed cert, a real `https.createServer` bound and listening,
// then a bare `https.request({ hostname, port, path, method })` with NO
// `.end()` even called yet):
//
//   [RTS PANIC] RefCell already borrowed
//     at crates\rts-core\src\entry\current.rs:220:34
//
//   Panic backtrace (the frames that carry real information; a few in the
//   middle are `with_current`'s own generic monomorphization, folded by the
//   linker onto an unrelated symbol name — read past those):
//     rts_node::net::common::get_value            <- panics HERE: a SECOND
//                                                     with_current/with_runtime
//                                                     borrow while one is
//                                                     already open
//     rts_node::tls::server::on_connection         <- server-side: a queued
//                                                     'connection' getting
//                                                     emitted...
//     rts_node::net::registry::pump                <- ...as a SIDE EFFECT of
//     rts_node::net::socket::write_hook               pumping the socket
//                                                     registry from INSIDE...
//     rts_node::stream::writable::write / write_done
//     rts_node::tls::socket::get_protocol          <- ...the CLIENT's own
//     rts_node::tls::socket::write_hook               getProtocol()/write(),
//                                                     called back-to-back in
//                                                     a tight Rust loop by...
//     rts_node::https::client::connect_blocking (this module's own function:
//       "Spins on tlsSocket.write(empty)... until getProtocol() reports the
//       handshake done" — crates/rts-node/src/https/client.rs)
//
// So the mechanism is real reentrancy, not a fluke: connect_blocking's own
// tight write()+getProtocol() loop, run with NO yield back to JS between
// iterations, ends up driving the SERVER side's own accept-queue pump
// (net::registry::pump) from inside the CLIENT side's write call — on the
// same OS thread, inside an ALREADY-OPEN runtime borrow — and the server's
// own 'connection' handler then opens a SECOND one. This is exactly the
// class CLAUDE.md's own "honesty floor" section warns is silent and
// recurring: "a rule applied in the wrong order" / a native calling back
// into the runtime while a borrow from an outer native is still open.
//
// A raw `tls.connect()` + `tls.createServer()` pair driven BY HAND from JS
// (one `socket.write("")` / `socket.getProtocol()` pair per statement, with
// real waits between them — see claude-node-tls-handshake-note below) does
// NOT crash this way; it just never completes the handshake either (see
// that separate finding). It is specifically https.client's OWN tight Rust
// loop that reproduces the reentrancy.
//
// THE KILLER CALL (left unrun — uncomment to reproduce; needs
// claude-node-https.test.ts's own CERT/KEY inlined the same way):
//
// import * as https from "node:https";
// import { time } from "rts";
// const server: any = https.createServer({ cert: CERT, key: KEY });
// server.listen(0, "127.0.0.1");
// time.sleep_ms(100);
// const port = server.__httpsTlsServer.__underlyingServer.address().port;
// https.request({ hostname: "127.0.0.1", port, path: "/", method: "GET" });
// // ^ never returns normally — the process panics before this line's own
// //   completion.
//
// ============================================================================
// KILLER CALL #2 — https.request() to an address NOTHING is listening on
// kills the process too, by a completely different, narrower mechanism:
// an uncaught 'error' EVENT, not a Rust panic.
// ============================================================================
//
// Read in `https/client.rs`'s own `build_request`: on a failed connect it
// builds the `ClientRequest` instance and calls
// `emit(instance, "error", error_instance, ...)` SYNCHRONOUSLY, THEN returns
// the instance to the caller. That ordering makes the failure
// unlisteneable-for by construction: a caller cannot possibly call
// `req.on('error', ...)` before the object attaching that listener to even
// EXISTS. Verified directly, twice — once bare, once wrapped in a
// `try/catch` around the whole `https.request(...)` call:
//
//   rts: uncaught 'error' event: an object     (process exit 1, both times)
//
// The try/catch case is the sharper proof: `catch` never ran and the
// process still exited 1, so this is not an ordinary catchable JS exception
// — the emit-with-no-listener path aborts the process directly, the same
// way an uncaught exception at the top level does elsewhere in this engine.
// Real Node's `http(s).request()` NEVER emits 'error' synchronously during
// construction for exactly this reason (a connection attempt is always
// asynchronous there), so the ordinary, universally-documented Node idiom —
// `const req = https.request(opts); req.on('error', cb); req.end();` — is
// unusable here for the single most common failure this API has to report:
// nothing listening on the other end.
//
// THE KILLER CALL (left unrun — no cert/key even needed, since the failure
// is at the TCP layer before TLS is reached):
//
// import * as https from "node:https";
// https.request({ hostname: "127.0.0.1", port: 1, path: "/", method: "GET" });
// // ^ port 1 refuses; the process exits 1 on this line, always, no matter
// //   what listener a caller tries to attach to the value it would have
// //   returned.

import { describe, test, expect } from "rts:test";

// The only thing this file safely measures: importing the module and
// reading its shape does not, by itself, trigger either killer call above.
import * as https from "node:https";
const importedSafelyOk = typeof https.request === "function";

describe("node:https — importing the module alone is safe (the crashes need a real call)", () => {
    test("node:https imports without crashing", () => expect(importedSafelyOk).toBe(true));
});
