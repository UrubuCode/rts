// node:http — a refused connection reaches `req.on('error')` instead of ending
// the process.
//
// What this pins, in one sentence: `http.request` and `https.request` open a
// socket of their OWN (`new net.Socket()`, `tls.connect`), nothing a program
// can write puts a listener on it, and an `'error'` with no listener is a
// throw — so a refused connection killed the process from inside
// `http.request(...)`, before the line that would have handled it.
//
// The failure that found it: `@whiskeysockets/baileys`. Its `ws` client does
// `new WebSocket(url)` and `ws`'s own `initAsClient` calls `https.request`,
// then attaches `req.on('error', …)` on the next statement. Measured
// 2026-10-02 with the binary of this tree: `new WebSocket("ws://127.0.0.1:1/x")`
// ended the process with `Error [ERR_UNHANDLED_ERROR]` whose `.stack` read
// `at initAsClient`, and a `try`/`catch` around the constructor did not save
// it — the throw came back out a second time after being caught. Against the
// live service the same shape fired on WhatsApp's own reset mid-handshake
// (`os error 10054`, delivered as `SocketEvent::Error` on that same socket).
//
// Node v22.23.2 on this machine, same options object and no `end()`:
//   http.request({host:"127.0.0.1",port:1,path:"/"})  → 'error', code ECONNREFUSED
//   https.request({host:"127.0.0.1",port:1,path:"/"}) → 'error', code ECONNREFUSED
//
// # Why there is no server in this file
//
// Port 1 is refused rather than listened to, which is the whole point: the case
// under test IS the failure, so it needs the absence of a server rather than
// the presence of one. Nothing leaves the loopback interface and no name is
// resolved.
//
// # What is NOT covered here, stated rather than implied
//
//   * `https.request`. It was changed by the same argument and measured the
//     same way — `https err: ECONNREFUSED` reaching the listener, where before
//     the change the process died — but it cannot be asserted in this FILE:
//     importing `node:http` and `node:https` into one program and failing a
//     request through each loses the first module's deferred `'error'`
//     entirely, and at port 1 raises `TypeError: object is not a function`
//     instead. That behaviour is IDENTICAL before and after this change
//     (checked against the pre-change binary with `port: 0`, where no socket
//     error exists at all and only the two `emit_error_later` deferrals run:
//     `https err` arrives, `http err` never does, both builds). So it is a
//     third defect, in whatever `emit_error_later`'s `setTimeout` closure
//     shares between the two modules, and a separate lot.
//   * `req.end()` AFTER a connection that failed. With this change the
//     `'error'` listener runs and reports ECONNREFUSED, and the process then
//     still dies with the same `TypeError` once the timer loop turns. That path
//     was unreachable before — the process died inside `http.request` — so this
//     too is a defect uncovered rather than introduced. The request below
//     therefore does not call `end()`, which Node does not need either.
//   * `net.connect` on its own. Its `'error'` is queued by a connector thread
//     and delivered by `net::registry::pump`, and `node:net` declares no loop
//     source — so a program that only waits never gets it (measured: Node
//     delivers ECONNREFUSED, this engine delivers nothing in 1.5 s). Another
//     defect, and not what this change touches.
//   * the error being a real `Error` instance. Node's is (`e instanceof Error`
//     is true); every error this crate's socket layer builds is a plain object
//     carrying `message` and `code`, which `net::registry::error_value` states
//     as a known limit of the entry surface. So this file asserts `code`, the
//     field a program branches on, and not the class.
//   * `ws` and baileys themselves, which need `node_modules` this suite does
//     not carry.
import { describe, test, expect } from "rts:test";
import * as http from "node:http";

// Settled by the listener the program attaches AFTER the call returns — which
// is the ordering the defect made impossible. No timeout guard inside the
// executor: `rts:test` fails a test whose promise never settles, which is the
// verdict a guard would only have invented.
const refused: Promise<any> = new Promise((resolve) => {
    const req: any = http.request({ host: "127.0.0.1", port: 1, path: "/" });
    req.on("error", (error: any) => resolve(error));
});

describe("node:http — a client's own socket failing is an 'error' on the request, not the end of the process", () => {
    test("a refused connection reaches req.on('error') at all — the internal net.Socket's unlistened 'error' used to kill the process from inside http.request", async () => {
        const error = await refused;
        expect(typeof error).toBe("object");
    });

    test("and it carries ECONNREFUSED — the socket's OWN code, not a guess: 'connect failed' with a hardcoded code erased which of refused, unreachable and timed out happened", async () => {
        const error = await refused;
        expect(String(error.code)).toBe("ECONNREFUSED");
    });

    test("and a message, so a log line says what happened instead of 'undefined'", async () => {
        const error = await refused;
        expect(typeof error.message).toBe("string");
        expect(error.message.length > 0).toBe(true);
    });
});
