// node:inspector — reproduces the API shape with no V8 backend behind it
// (crates/rts-node/src/inspector/mod.rs's own `//!`, per inspector.md §5.1
// scope (b)): what is real is real (a loopback TcpListener + discovery
// responder for open/close/url/waitForDebugger; Session.post for
// Runtime.evaluate, bridged to entry::evaluate; getHeapUsage over the same
// primitive node:v8 uses), and everything else answers ERR_INSPECTOR_COMMAND
// by name instead of a fabricated result.
//
// waitForDebugger() genuinely BLOCKS the calling thread until a real
// connection arrives — this fixture never calls it against an endpoint with
// nobody going to connect, per this task's own "don't hang the process"
// rule. What it DOES test: waitForDebugger() is a documented no-op when
// nothing is open (safe), and a real client that connects unblocks it (using
// node:net to dial the loopback port from... this same single-threaded
// engine cannot do that concurrently, so that half is left as a named gap
// below rather than attempted unsafely).

import { describe, test, expect } from "rts:test";
import * as inspector from "node:inspector";

// --- open/url/close over a real loopback listener ---------------------------
const urlBeforeOpen = inspector.url();
const urlBeforeOpenOk = urlBeforeOpen === undefined;

// waitForDebugger() with nothing open is a documented no-op — safe to call.
const noopReturn = inspector.waitForDebugger();
const noopReturnOk = noopReturn === undefined;

inspector.open(0, undefined, false); // port 0 = OS picks a free one; wait=false
const urlAfterOpen = inspector.url();
const urlIsWsOk = typeof urlAfterOpen === "string" && urlAfterOpen!.indexOf("ws://127.0.0.1:") === 0;
const urlHasRealPortOk = (() => {
    const match = /^ws:\/\/127\.0\.0\.1:(\d+)\//.exec(urlAfterOpen!);
    return match !== null && Number(match[1]) > 0;
})();

// A second open() while one is active is refused (per the endpoint's own
// doc) — readable only through url() staying the SAME address, since open()
// has no error channel to report through in Node either.
inspector.open(0, undefined, false);
const urlUnchangedBySecondOpen = inspector.url() === urlAfterOpen;

inspector.close();
const urlAfterCloseOk = inspector.url() === undefined;
// close() a second time (nothing open) does not throw/hang.
inspector.close();
const secondCloseOk = true;

// `open()`'s `host` argument is accepted and ignored — the bind is ALWAYS
// loopback, verified by asking for a non-loopback host and reading url() back.
inspector.open(0, "0.0.0.0", false);
const stillLoopbackOk = inspector.url()!.indexOf("ws://127.0.0.1:") === 0;
inspector.close();

// --- new Session() + post() --------------------------------------------------
const session = new inspector.Session();
const sessionTypeOk = typeof session === "object";
const connectIsSelf = session.connect() === undefined; // no return value documented
session.disconnect();
const disconnectOk = true; // does not throw

// Real acknowledgements: enable/disable answer an empty result, no error.
let enableErr: any = "unset";
let enableResult: any = null;
session.post("Runtime.enable", {}, (err: any, result: any) => {
    enableErr = err;
    enableResult = result;
});
const enableResultOk = typeof enableResult === "object";
// FINDING, isolated on its own: real Node's error-first callback answers
// `null` on success (verified directly against real Node v20). This engine's
// `post` answers `undefined` instead — read in `inspector/session.rs`:
// `Answer::Value(value) => (absent, value)`, where `absent` is the ABI's
// "no argument" placeholder (`entry::undefined_value()`), passed straight
// through as the error argument rather than a real `null`. A caller that
// checks `err === null` (the standard, documented Node idiom — see e.g.
// Node's own inspector docs) never takes the success branch here. Asserting
// Node's real answer; expected to stay RED.
const errIsRealNullOk = enableErr === null;

// Runtime.evaluate — bridged to entry::evaluate, the same seam node:vm runs
// on. A plain arithmetic expression is a value that can cross that boundary.
let evalResult: any = null;
session.post("Runtime.evaluate", { expression: "6 * 7" }, (_err: any, result: any) => {
    evalResult = result;
});
const evalOk = evalResult && evalResult.result && evalResult.result.value === 42;

// Runtime.getHeapUsage — the same region.capacity()/used() primitive
// node:v8's getHeapStatistics reports.
let heapResult: any = null;
session.post("Runtime.getHeapUsage", {}, (_err: any, result: any) => {
    heapResult = result;
});
const heapOk =
    typeof heapResult.usedSize === "number" &&
    typeof heapResult.totalSize === "number" &&
    heapResult.totalSize >= heapResult.usedSize;

// Schema.getDomains — only the domains actually backed, not Node's full list.
let domainsResult: any = null;
session.post("Schema.getDomains", {}, (_err: any, result: any) => {
    domainsResult = result;
});
const domainNames = (domainsResult?.domains ?? []).map((d: any) => d.name);
const domainsOk =
    domainNames.indexOf("Runtime") >= 0 &&
    domainNames.indexOf("Debugger") >= 0 &&
    domainNames.indexOf("HeapProfiler") >= 0 &&
    domainNames.indexOf("Schema") >= 0;

// A method outside the allowlist refuses by name — never a crash, never a
// fabricated result.
let refusedErr: any = null;
let refusedResult: any = null;
session.post("Network.enable", {}, (err: any, result: any) => {
    refusedErr = err;
    refusedResult = result;
});
const refusedOk =
    refusedErr !== null && refusedErr.code === "ERR_INSPECTOR_COMMAND" && refusedResult === undefined;

// Profiler.start/HeapProfiler.takeHeapSnapshot refuse by name — no sampling
// profiler exists in this engine, and an empty profile would be a fabricated
// result the module's own doc refuses to produce.
let profilerErr: any = null;
session.post("Profiler.start", {}, (err: any) => {
    profilerErr = err;
});
const profilerRefusedOk = profilerErr !== null && profilerErr.code === "ERR_INSPECTOR_COMMAND";

// post(method, callback) — the two-argument overload (no params object).
let twoArgResult: any = null;
session.post("Runtime.enable", (_err: any, result: any) => {
    twoArgResult = result;
});
const twoArgOk = typeof twoArgResult === "object";

describe("node:inspector — open/url/close over a real loopback listener", () => {
    test("url() answers undefined before open()", () => expect(urlBeforeOpenOk).toBe(true));
    test("waitForDebugger() is a safe no-op when nothing is open", () => expect(noopReturnOk).toBe(true));
    test("open(0) binds a real ephemeral port and url() reports ws://127.0.0.1:<port>/<id>", () => {
        expect(urlIsWsOk).toBe(true);
        expect(urlHasRealPortOk).toBe(true);
    });
    test("a second open() while active does not change the bound address", () =>
        expect(urlUnchangedBySecondOpen).toBe(true));
    test("close() clears url() back to undefined", () => expect(urlAfterCloseOk).toBe(true));
    test("a second close() with nothing open does not throw", () => expect(secondCloseOk).toBe(true));
    test("open()'s host argument is accepted and ignored — always loopback", () =>
        expect(stillLoopbackOk).toBe(true));
});

describe("node:inspector — new Session() + post()", () => {
    test("new Session() returns an object; connect()/disconnect() do not throw", () => {
        expect(sessionTypeOk).toBe(true);
        expect(connectIsSelf).toBe(true);
        expect(disconnectOk).toBe(true);
    });
    test("Runtime.enable acknowledges with an empty result object", () => expect(enableResultOk).toBe(true));
    test("on success the callback's error argument is real `null`, per Node's documented convention", () =>
        expect(errIsRealNullOk).toBe(true));
    test("Runtime.evaluate runs real source through entry::evaluate", () => expect(evalOk).toBe(true));
    test("Runtime.getHeapUsage reports real heap numbers", () => expect(heapOk).toBe(true));
    test("Schema.getDomains lists only the domains actually backed", () => expect(domainsOk).toBe(true));
    test("an unimplemented method refuses with ERR_INSPECTOR_COMMAND, not a crash or a fake result", () =>
        expect(refusedOk).toBe(true));
    test("Profiler.start refuses by name (no sampling profiler exists)", () => expect(profilerRefusedOk).toBe(true));
    test("post(method, callback) — the 2-argument overload — reaches the same dispatch", () =>
        expect(twoArgOk).toBe(true));
});
