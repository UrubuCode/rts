// node:events — events.on(emitter, name), the async-iterator sibling of
// events.once. Unlike events.once, this one does NOT crash (see
// claude-node-events-once-crash.test.ts): its own listener calls
// `packed_args` OUTSIDE any `with_runtime` block, so it never nests the
// borrow. Every scenario below was checked against `node -e` at the same
// time this file was written and matches Node's answer exactly.
//
// ORDERING NOTE, same rule as events.once (no event loop here): emit before
// the `await it.next()` you expect it to satisfy, either on the same
// synchronous turn (subscribe, emit N times, then await N times — the
// buffer absorbs the ones that arrive before anyone asks) or from a
// `setTimeout` callback that runs while a `next()` is already parked.
// Confirmed working: both an all-synchronous burst (three emits, then three
// `await next()`) and a `next()` parked BEFORE a deferred
// `setTimeout(() => e.emit(...), 5)`.
import { describe, test, expect } from "rts:test";
import { EventEmitter } from "node:events";
import * as events from "node:events";

// --- burst then drain: buffering absorbs events nobody has awaited yet ----
async function runBurst() {
    const e = new EventEmitter();
    const it = events.on(e, "x");
    e.emit("x", 1);
    e.emit("x", 2);
    e.emit("x", 3);
    const r1 = await it.next();
    const r2 = await it.next();
    const r3 = await it.next();
    await (it as any).return();
    return { r1, r2, r3 };
}
const burstPromise = runBurst();

// --- parked next() resolved by a deferred emit ------------------------------
async function runDeferred() {
    const e = new EventEmitter();
    const it = events.on(e, "x");
    setTimeout(() => { e.emit("x", "deferred"); }, 5);
    const r1 = await it.next(); // parked: nothing buffered yet when this runs
    await (it as any).return();
    return r1;
}
const deferredPromise = runDeferred();

// --- 'error' rejects the next `next()`, but only after buffered items drain
async function runErrorAfterBuffered() {
    const e = new EventEmitter();
    const it = events.on(e, "x");
    e.emit("x", "first");
    e.emit("error", new Error("kaboom"));
    e.emit("x", "second"); // never reached: listeners already removed by on_error
    const r1 = await it.next();
    let caughtMsg = "";
    try {
        await it.next();
    } catch (err: any) {
        caughtMsg = err.message;
    }
    return { r1, caughtMsg };
}
const errorPromise = runErrorAfterBuffered();

// --- return() ends iteration and resolves a parked next() to {done:true} --
async function runReturnResolvesParked() {
    const e = new EventEmitter();
    const it = events.on(e, "x");
    const parked = it.next(); // nothing buffered — parks a promise
    const returned = await (it as any).return();
    const parkedResult = await parked;
    return { returned, parkedResult };
}
const returnPromise = runReturnResolvesParked();

// --- for await ... of consumes the buffer in order --------------------------
async function runForAwait() {
    const e = new EventEmitter();
    const it = events.on(e, "tick");
    e.emit("tick", "a");
    e.emit("tick", "b");
    setTimeout(() => { (it as any).return(); }, 5);
    const seen: string[] = [];
    for await (const [value] of it as any) {
        seen.push(value);
    }
    return seen;
}
const forAwaitPromise = runForAwait();

const all = Promise.all([burstPromise, deferredPromise, errorPromise, returnPromise, forAwaitPromise]);

describe("node:events events.on — buffering", () => {
    test("three emits before any next() are all buffered, in order", async () => {
        const { r1, r2, r3 } = await burstPromise;
        expect(JSON.stringify(r1)).toBe(JSON.stringify({ value: [1], done: false }));
        expect(JSON.stringify(r2)).toBe(JSON.stringify({ value: [2], done: false }));
        expect(JSON.stringify(r3)).toBe(JSON.stringify({ value: [3], done: false }));
    });
});

describe("node:events events.on — deferred emit resolves a parked next()", () => {
    test("next() parked before a setTimeout-driven emit resolves it", async () => {
        const r1 = await deferredPromise;
        expect(JSON.stringify(r1)).toBe(JSON.stringify({ value: ["deferred"], done: false }));
    });
});

describe("node:events events.on — 'error' ends iteration after buffered items drain", () => {
    test("buffered item before the error is still delivered first", async () => {
        const { r1 } = await errorPromise;
        expect(JSON.stringify(r1)).toBe(JSON.stringify({ value: ["first"], done: false }));
    });
    test("the following next() rejects with the error", async () => {
        const { caughtMsg } = await errorPromise;
        expect(caughtMsg).toBe("kaboom");
    });
});

describe("node:events events.on — return()", () => {
    test("return() resolves to {done:true}", async () => {
        const { returned } = await returnPromise;
        expect(JSON.stringify(returned)).toBe(JSON.stringify({ value: undefined, done: true }));
    });
    test("a next() parked before return() also resolves to {done:true}", async () => {
        const { parkedResult } = await returnPromise;
        expect(JSON.stringify(parkedResult)).toBe(JSON.stringify({ value: undefined, done: true }));
    });
});

describe("node:events events.on — for await...of", () => {
    test("iterates buffered values in order then stops on return()", async () => {
        const seen = await forAwaitPromise;
        expect(seen.join(",")).toBe("a,b");
    });
});
