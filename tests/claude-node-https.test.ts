// node:https — createServer/Server, request/get, Agent/globalAgent
// (crates/rts-node/src/https/, 576L). Never imported by any fixture before
// this one. The module's own `//!` doc claims it reuses `node:http`'s
// parser/IncomingMessage/ServerResponse and `node:tls`'s handshake wholesale
// — measured true by direct code reading (no second parser exists in this
// file) — but the doc ALSO claims things this fixture found are not quite
// so; see each finding below.
//
// A REAL end-to-end request against a REAL httpS server is attempted, and
// it does not complete: it panics the whole process (`RefCell already
// borrowed`, a nested-borrow abort — exactly the failure class this crate's
// own conventions name repeatedly). That, and a second, narrower crash (any
// `https.request()` to an address nothing is listening on), are isolated in
// `claude-node-https-crash.test.ts` instead of here, left UNRUN with the
// exact repro recorded, so this file stays green where the module actually
// is.
//
// A real self-signed EC P-256 certificate is used throughout (this crate's
// TLS provider signs with ECDSA P-256/Ed25519 only, per
// `tls/provider/mod.rs` — RSA signing is not implemented), generated once
// via `openssl ecparam`/`openssl pkcs8 -topk8` for `CN=localhost` and
// inlined as plain PEM text.

import { describe, test, expect } from "rts:test";
import * as https from "node:https";
import * as http from "node:http";
import { time } from "rts";

const CERT = `-----BEGIN CERTIFICATE-----
MIIBfTCCASOgAwIBAgIUDjpUL+614zpP6h+C0TYh5iN2mSYwCgYIKoZIzj0EAwIw
FDESMBAGA1UEAwwJbG9jYWxob3N0MB4XDTI2MDkwMzAzMjMzMFoXDTM2MDgzMTAz
MjMzMFowFDESMBAGA1UEAwwJbG9jYWxob3N0MFkwEwYHKoZIzj0CAQYIKoZIzj0D
AQcDQgAEvL/xTm6wkJxyk/BdBUD9KDLqw+Wm8CmCpxyZ0PnKx6R7DWhrYBlGWUNj
unvq3Ati8gsDeT3kEgojfZ+45VYQdKNTMFEwHQYDVR0OBBYEFOPUy0KJAMoVCpdh
nPycE2JbDNx5MB8GA1UdIwQYMBaAFOPUy0KJAMoVCpdhnPycE2JbDNx5MA8GA1Ud
EwEB/wQFMAMBAf8wCgYIKoZIzj0EAwIDSAAwRQIhALfY30Zxt19nzqWIdhJlsBzm
DiwO6n+0dv5Z+B9o0q/pAiAsvhc+7XvAhxNK0tCbLT0wbq6gpyOR4Dt3bvlkbw4s
Rg==
-----END CERTIFICATE-----`;
const KEY = `-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgorNcjTI2rG2tUjww
L8hTbWQLCraIy5MQAq8ez6Ss/SuhRANCAAS8v/FObrCQnHKT8F0FQP0oMurD5abw
KYKnHJnQ+crHpHsNaGtgGUZZQ2O6e+rcC2LyCwN5PeQSCiN9n7jlVhB0
-----END PRIVATE KEY-----`;

// --- namespace shape ----------------------------------------------------------
const shapeOk =
    typeof https.createServer === "function" &&
    typeof https.Server === "function" &&
    typeof https.request === "function" &&
    typeof https.get === "function" &&
    typeof https.Agent === "function" &&
    typeof https.globalAgent === "object";

// --- server construction + method surface --------------------------------
const server: any = https.createServer({ cert: CERT, key: KEY }, (_req: any, _res: any) => {});
const serverTypeOk = typeof server === "object";
const listenIsFnOk = typeof server.listen === "function";
const closeIsFnOk = typeof server.close === "function";
const onIsFnOk = typeof server.on === "function";

// FINDING: `server.address` does not exist AT ALL — not merely "before
// listen", never. Root cause, read directly: `http.Server` (which
// `https.createServer` builds internally and returns AS the public object —
// see the module's own doc, "(1) builds a real http.Server by calling
// http's own constructor") holds its own `net.Server` only through
// individually-forwarded methods (`http/server.rs`'s own METHODS table:
// listen/close/closeAllConnections/closeIdleConnections/setTimeout) rather
// than real prototype inheritance from `net.Server` — and `address` is not
// one of the forwarded names. This is an `http.Server` gap `node:https`
// simply inherits, not something https adds on its own; verified directly
// against real Node, where `server.address()` is a documented, ordinary
// method on both. Expected to stay RED.
const addressIsFnOk = typeof server.address === "function";

server.listen(0, "127.0.0.1");
time.sleep_ms(80); // accept thread needs a moment to actually bind
// listen() IS real: the bound port is readable through the internal TLS
// server object's own underlying net.Server (the one https.Server's
// shadowed `listen` actually routes to — see the module's own doc). Reached
// structurally here only to PROVE listen() bound something real; ordinary
// code has no reason to reach this deep.
const boundPort = server.__httpsTlsServer.__underlyingServer.address().port;
const reallyBoundOk = typeof boundPort === "number" && boundPort > 0;
server.close();

// --- Agent / globalAgent ----------------------------------------------------
// The module's own doc: "This module hands back http's own Agent class
// directly — same constructor, same prototype, same instance shape."
// Measured: the PROTOTYPE claim holds (and so does the practical,
// Node-compatible `instanceof` relationship) — but the CONSTRUCTOR itself is
// a fresh callable, not http.Agent verbatim. Root cause, read directly:
// `https::agent::agent_ctor` reaches `node:http`'s Agent through
// `http_member`, which calls `crate::http::namespace(context)` — a Rust-level
// rebuild that mints a brand-new `Agent` callable via `make_callable` every
// time it runs (unlike a prototype, which `make_prototype` DOES memoize by
// name), so the object a JS-level `import "node:http"` sees is a DIFFERENT
// callable than the one https captured when IT called the same builder.
const prototypeSharedOk = (https.Agent as any).prototype === (http.Agent as any).prototype;
const instanceofOk = new (https.Agent as any)() instanceof (http.Agent as any);
const globalAgentInstanceofOk = https.globalAgent instanceof (http.Agent as any);
// Asserting the module's own literal claim ("same constructor") — expected
// RED, contrasted with the three green ones just above.
const sameConstructorOk = https.Agent === http.Agent;

describe("node:https — namespace shape", () => {
    test("createServer/Server/request/get/Agent/globalAgent all exist with the right typeof", () =>
        expect(shapeOk).toBe(true));
});

describe("node:https — server construction over a real self-signed cert", () => {
    test("createServer(options, listener) returns an object with listen/close/on", () => {
        expect(serverTypeOk).toBe(true);
        expect(listenIsFnOk).toBe(true);
        expect(closeIsFnOk).toBe(true);
        expect(onIsFnOk).toBe(true);
    });
    test("listen(0, host) really binds a real ephemeral port", () => expect(reallyBoundOk).toBe(true));
    test("server.address is a real method, per Node (this engine: missing entirely)", () =>
        expect(addressIsFnOk).toBe(true));
});

describe("node:https — Agent/globalAgent vs. the module's own doc", () => {
    test("https.Agent.prototype === http.Agent.prototype", () => expect(prototypeSharedOk).toBe(true));
    test("new https.Agent() instanceof http.Agent (Node-compatible even though real Node subclasses)", () =>
        expect(instanceofOk).toBe(true));
    test("https.globalAgent instanceof http.Agent", () => expect(globalAgentInstanceofOk).toBe(true));
    test("https.Agent === http.Agent, per the module's OWN doc ('hands back http's own Agent class directly')", () =>
        expect(sameConstructorOk).toBe(true));
});
