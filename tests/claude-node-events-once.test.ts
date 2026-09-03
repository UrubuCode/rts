// node:events — events.once(emitter, name, options?), the paths that do NOT
// crash the process. See claude-node-events-once-crash.test.ts for the one
// that does: the SUCCESS path (the awaited event actually firing) crashes
// the process every time, so it cannot be exercised here — this file covers
// only rejection (the 'error' listener, options.signal) and up-front refusal
// (an invalid emitter), which never reach the broken code path.
//
// ORDERING NOTE for whoever reaches for `events.once` next: there is no
// event loop here, so `await events.once(e, "x")` followed by `e.emit("x")`
// on the NEXT line never runs the emit before the await parks — you must
// emit BEFORE the `await`, on the same synchronous turn (subscribe, emit,
// *then* await the already-registered promise), or emit from something that
// runs while the await is parked (a setTimeout callback works: confirmed
// live that `events.once` resolves from a deferred `setTimeout(() =>
// e.emit(...), 5)` fired while the await is parked — see
// claude-node-events-once-crash.test.ts, which uses exactly that shape to
// prove the crash is not a same-turn artifact). None of that matters for
// the paths below, since none of them go through the crashing success path.
import { describe, test, expect } from "rts:test";
import { EventEmitter } from "node:events";
import * as events from "node:events";

// --- rejects on 'error' (the awaited event is something else) -------------
// Confirmed against Node: emitting 'error' while `events.once(e, "data")` is
// pending rejects that promise with the error value itself.
let errRejectMsg = "";
let errRejectHappened = false;
async function runErrorReject() {
    const e = new EventEmitter();
    const p = events.once(e, "data");
    e.emit("error", new Error("boom"));
    try {
        await p;
    } catch (err: any) {
        errRejectHappened = true;
        errRejectMsg = err.message;
    }
}
const errRejectPromise = runErrorReject();

// --- already-aborted signal rejects immediately ----------------------------
let abortImmediateName = "";
let abortImmediateMsg = "";
let abortImmediateCode = "";
let abortImmediateHappened = false;
async function runAbortImmediate() {
    const e = new EventEmitter();
    const ac = new AbortController();
    ac.abort();
    try {
        await events.once(e, "x", { signal: ac.signal });
    } catch (err: any) {
        abortImmediateHappened = true;
        abortImmediateName = err.name;
        abortImmediateMsg = err.message;
        abortImmediateCode = String(err.code);
    }
}
const abortImmediatePromise = runAbortImmediate();

// --- signal aborted after the promise is pending ---------------------------
let abortLaterHappened = false;
let abortLaterName = "";
async function runAbortLater() {
    const e = new EventEmitter();
    const ac = new AbortController();
    const p = events.once(e, "y", { signal: ac.signal });
    ac.abort();
    try {
        await p;
    } catch (err: any) {
        abortLaterHappened = true;
        abortLaterName = err.name;
    }
}
const abortLaterPromise = runAbortLater();

// --- invalid emitter is refused synchronously, not silently ----------------
// events.once returns a promise that REJECTS (not a thrown synchronous
// error) — confirmed live: `ERR_INVALID_ARG_TYPE`, `TypeError`, matching
// real Node's message shape ("must be an instance of EventEmitter" in Node
// vs RTS's "must be of type EventEmitter" — the report notes the wording
// difference; this test only pins name/code, which agree).
let invalidEmitterCode = "";
let invalidEmitterName = "";
let invalidEmitterHappened = false;
async function runInvalidEmitter() {
    try {
        await (events as any).once({}, "x");
    } catch (err: any) {
        invalidEmitterHappened = true;
        invalidEmitterCode = err.code;
        invalidEmitterName = err.name;
    }
}
const invalidEmitterPromise = runInvalidEmitter();

// --- invalid event name (an object, not a string/symbol) -------------------
let invalidNameCode = "";
let invalidNameHappened = false;
async function runInvalidName() {
    const e = new EventEmitter();
    try {
        await (events as any).once(e, {});
    } catch (err: any) {
        invalidNameHappened = true;
        invalidNameCode = err.code;
    }
}
const invalidNamePromise = runInvalidName();

const all = Promise.all([
    errRejectPromise,
    abortImmediatePromise,
    abortLaterPromise,
    invalidEmitterPromise,
    invalidNamePromise,
]);

describe("node:events events.once — rejection paths", () => {
    test("rejects with the 'error' value", async () => {
        await all;
        expect(errRejectHappened).toBe(true);
        expect(errRejectMsg).toBe("boom");
    });
    test("already-aborted signal rejects immediately, AbortError", async () => {
        await all;
        expect(abortImmediateHappened).toBe(true);
        expect(abortImmediateName).toBe("AbortError");
    });
    // NOTE (report, not asserted red here to keep this file green where the
    // behavior IS observable): Node's message is "The operation was
    // aborted" and err.code is the string "ABORT_ERR"; RTS answers "This
    // operation was aborted" and err.code is the NUMBER 20 (the DOMException
    // numeric code) — confirmed live both ways. Node's AbortError is a plain
    // Error (not instanceof DOMException); RTS's IS instanceof DOMException.
    test("later-abort (signal aborts after subscribe) rejects, AbortError", async () => {
        await all;
        expect(abortLaterHappened).toBe(true);
        expect(abortLaterName).toBe("AbortError");
    });
    test("invalid emitter rejects ERR_INVALID_ARG_TYPE TypeError", async () => {
        await all;
        expect(invalidEmitterHappened).toBe(true);
        expect(invalidEmitterCode).toBe("ERR_INVALID_ARG_TYPE");
        expect(invalidEmitterName).toBe("TypeError");
    });
    test("invalid event name rejects ERR_INVALID_ARG_TYPE", () => {
        expect(invalidNameHappened).toBe(true);
        expect(invalidNameCode).toBe("ERR_INVALID_ARG_TYPE");
    });
});
