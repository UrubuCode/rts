// node:http / node:https request options — an option that is NOT THERE is not
// the string "undefined".
//
// What this pins, in one sentence: `option_text` in `http/common.rs`,
// `https/common.rs` and `tls/common.rs` converted the option's value with
// `ToString` instead of TESTING that it is a string, so a property the caller
// never wrote came back as the nine-letter text `"undefined"` and won over the
// property beside it.
//
// The failure that found it: `@whiskeysockets/baileys` could not open its
// WebSocket. `ws` builds its request options from a defaults object that
// spreads `hostname: undefined` in (`node_modules/ws/lib/websocket.js`,
// `initAsClient`) and then sets `host`; `https::client::apply_options` reads
// `hostname` first, got `"undefined"`, and every `wss://` handshake dialled a
// host by that name — `os error 11001`, WSAHOST_NOT_FOUND — while
// `dns.lookup("web.whatsapp.com", ...)` answered `57.144.179.32` one call
// away. And it was never only about an explicitly-undefined key: an ABSENT
// `hostname` read the same way, so `http.request({ host: "127.0.0.1" })` had
// never once dialled 127.0.0.1.
//
// # Why there is no socket in this file
//
// Every assertion below is read off the `ClientRequest` object `http.request`
// returns, with `port: 0` so that `net`'s own `connect` declines before any OS
// call (`net/socket.rs`: `if port == 0 { return this }`) — no name is
// resolved, no packet leaves, nothing listens. `http::client::build_request`
// sets `host`/`path`/`method` on the instance BEFORE it connects, which is
// what makes the reader observable without a server.
//
// The failing connection still emits `'error'` on a later turn, so every
// request here attaches a listener; an `'error'` with nothing attached ends
// the process.
//
// NOT covered here, and stated rather than implied: the `https` and `tls`
// copies of the same reader. `https::client::build_request` sets `host` on the
// instance only on the path where the TLS handshake already succeeded, and
// `tls.connect` exposes neither its host nor its SNI name, so observing either
// needs a completed TLS handshake — which this engine's JS-visible
// `tls.connect` does not reach against a real server today (measured
// 2026-10-02: 221 087 pump writes in 8 s, `getProtocol()` still null). Both
// were changed by the same argument as `http`'s, and both are unmeasured by
// this file.
//
// Every "Node says" value below was measured on this machine with Node
// v22.23.2 against the identical options objects.
import { describe, test, expect } from "rts:test";
import * as http from "node:http";

function request(options: any): any {
    const req: any = http.request(options);
    // `port: 0` is a connect that cannot succeed; the deferred `'error'` has
    // to land somewhere.
    req.on("error", () => {});
    return req;
}

// ---- 1. `hostname: undefined` beside a real `host` ---------------------
// Node v22.23.2: req.host === "127.0.0.1". This is `ws`'s exact shape.
const explicitUndefined = request({ hostname: undefined, host: "127.0.0.1", port: 0, path: "/x", method: "GET" });

// ---- 2. no `hostname` key at all --------------------------------------
// Node v22.23.2: req.host === "127.0.0.1". The broader half of the defect —
// an absent property and a property holding `undefined` read identically.
const noHostnameKey = request({ host: "127.0.0.1", port: 0, path: "/x", method: "POST" });

// ---- 3. neither `hostname` nor `host` ---------------------------------
// Node v22.23.2: req.host === "localhost". The default has to be reachable,
// and it was not: "undefined" won before the default was ever consulted.
const neitherKey = request({ port: 0, path: "/x" });

// ---- 4. `path: undefined` ---------------------------------------------
// Node v22.23.2: req.path === "/". A request line reading `GET undefined
// HTTP/1.1` is not a 404, it is a malformed request.
const pathUndefined = request({ host: "127.0.0.1", port: 0, path: undefined, method: "GET" });

// ---- 5. `method: undefined` -------------------------------------------
// Node v22.23.2: req.method === "GET". `apply_options` upper-cases whatever
// it reads, so this one shipped the method "UNDEFINED" — a verb no server
// implements.
const methodUndefined = request({ host: "127.0.0.1", port: 0, path: "/x", method: undefined });

// ---- 6. the options that ARE written still win ------------------------
// The guard against fixing this by ignoring the field: a real `hostname`
// must still beat a real `host`, which is Node's documented precedence.
const hostnameWins = request({ hostname: "127.0.0.2", host: "127.0.0.1", port: 0, path: "/y", method: "PUT" });

describe("node:http request options — an absent option is absent, not the text \"undefined\"", () => {
    test("hostname: undefined beside host: '127.0.0.1' reads the host — ws spreads that key, and every wss:// handshake dialled a host called undefined", () => {
        expect(explicitUndefined.host).toBe("127.0.0.1");
    });

    test("no hostname key at all reads host — http.request({host}) had never dialled the host it was given", () => {
        expect(noHostnameKey.host).toBe("127.0.0.1");
        expect(noHostnameKey.method).toBe("POST");
    });

    test("neither hostname nor host falls back to localhost — the default was unreachable behind the conversion", () => {
        expect(neitherKey.host).toBe("localhost");
    });

    test("path: undefined is the path '/' — not a request line reading GET undefined HTTP/1.1", () => {
        expect(pathUndefined.path).toBe("/");
    });

    test("method: undefined is GET — it was the verb UNDEFINED, upper-cased on the way out", () => {
        expect(methodUndefined.method).toBe("GET");
    });

    test("a hostname that IS written still beats host, and path/method survive — the fix is a type test, not a dropped field", () => {
        expect(hostnameWins.host).toBe("127.0.0.2");
        expect(hostnameWins.path).toBe("/y");
        expect(hostnameWins.method).toBe("PUT");
    });
});
