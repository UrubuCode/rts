// node:events — EventEmitter surface completed in the split of the 769-line
// events.rs into crates/rts-node/src/events/: rawListeners()'s real wrapper,
// prepend order, max-listeners, eventNames, and the EventTarget/Event/
// CustomEvent re-export the module doc claims. Every assertion here was
// checked against `node -e "..."` (Node v20.19.5) at the same time this file
// was written; anywhere RTS disagrees the comment above the test says what
// Node answers and this test still asserts NODE's answer, not RTS's.
import { describe, test, expect } from "rts:test";
import { EventEmitter, EventTarget, Event, CustomEvent } from "node:events";
import * as events from "node:events";

// --- rawListeners(): the real once-wrapper, not a copy --------------------
// Node: rawListeners()[0] !== originalFn, but .listener === originalFn;
// listeners()[0] === originalFn always (unwrapped). Confirmed live via
// `node -e` against EventEmitter directly.
function fnA() {}
const eRaw = new EventEmitter();
eRaw.once("x", fnA);
const rawArr = eRaw.rawListeners("x");
const rawIsNotFn = rawArr[0] !== fnA;
const rawListenerIsFn = (rawArr[0] as any).listener === fnA;
const listenersIsFn = eRaw.listeners("x")[0] === fnA;
const countBeforeOff = eRaw.listenerCount("x");
eRaw.off("x", fnA);
const countAfterOffByOriginal = eRaw.listenerCount("x");

// off() also matches by the wrapper itself (rawListeners()[0]), not only by
// the original function — the module doc's stated reason `once_wrapper`
// carries `.listener` at all.
function fnB() {}
const eRawWrap = new EventEmitter();
eRawWrap.once("y", fnB);
const wrapper = eRawWrap.rawListeners("y")[0];
const countBeforeOffWrap = eRawWrap.listenerCount("y");
eRawWrap.off("y", wrapper as any);
const countAfterOffByWrapper = eRawWrap.listenerCount("y");

// --- prepend / prependOnce ordering ----------------------------------------
// on(a) -> prependListener(Z) -> once(appended) -> prependOnceListener(Y)
// gives call order Y, Z, a, appended. Confirmed against real Node.
let order = "";
const eOrder = new EventEmitter();
eOrder.on("go", () => { order += "a"; });
eOrder.prependListener("go", () => { order += "Z"; });
eOrder.once("go", () => { order += "once-appended"; });
eOrder.prependOnceListener("go", () => { order += "Y"; });
eOrder.emit("go");

// --- eventNames(): RTS gives pure insertion order; Node interleaves -------
// BUG (confirmed against real Node): Node's eventNames() puts every STRING
// key first (in insertion order), THEN every SYMBOL key (in insertion
// order) — that is plain JS own-key ordering (Reflect.ownKeys) applied to
// the internal `_events` object, not registration order. RTS's eventNames()
// just returns __eventNames__ in raw insertion order, so mixing a Symbol
// registered before a string comes out in the WRONG place.
//
// `node -e`:
//   e.on(s1,fn); e.on("b",fn); e.on(s2,fn); e.on("a",fn);
//   e.eventNames() -> ['b', 'a', Symbol(s1), Symbol(s2)]
// RTS answers ['Symbol(s1)', 'b', 'Symbol(s2)', 'a'] (raw insertion order).
// This assertion states NODE's answer and is expected to be RED on RTS.
const eNames = new EventEmitter();
const s1 = Symbol("s1");
const s2 = Symbol("s2");
eNames.on(s1, () => {});
eNames.on("b", () => {});
eNames.on(s2, () => {});
eNames.on("a", () => {});
const namesOrder = eNames.eventNames().map((n) => String(n)).join(",");
const namesOrderExpected = "b,a,Symbol(s1),Symbol(s2)";

// eventNames() length and basic content (not order) — sanity check that
// still passes regardless of the ordering bug above.
const eNames2 = new EventEmitter();
eNames2.on("alpha", () => {});
eNames2.on("beta", () => {});
const namesLen = eNames2.eventNames().length;

// --- getMaxListeners / setMaxListeners (instance) --------------------------
const eMax = new EventEmitter();
const defMax = eMax.getMaxListeners();
eMax.setMaxListeners(25);
const newMax = eMax.getMaxListeners();

// --- module-level static getMaxListeners/setMaxListeners(n, target) -------
const eStaticMax = new EventEmitter();
(events as any).setMaxListeners(50, eStaticMax);
const staticMaxViaInstance = eStaticMax.getMaxListeners();
const staticMaxViaModule = (events as any).getMaxListeners(eStaticMax);

// --- EventEmitter.defaultMaxListeners, mutable and shared -----------------
const defaultBefore = (EventEmitter as any).defaultMaxListeners;
(EventEmitter as any).defaultMaxListeners = 5;
const eAfterDefaultChange = new EventEmitter();
const maxAfterDefaultChange = eAfterDefaultChange.getMaxListeners();
(EventEmitter as any).defaultMaxListeners = defaultBefore; // restore for later tests in this file

// --- module.listenerCount(emitter, name) — deprecated static, Node HAS it -
// GAP (confirmed against real Node): `require("events").listenerCount` is a
// real (deprecated) function in Node — `typeof events.listenerCount ===
// "function"`, and `events.listenerCount(e, "x")` answers the count. RTS's
// namespace() member list omits "listenerCount" entirely, so this is
// `undefined` in RTS. Asserting Node's answer; expected RED on RTS.
const eLC = new EventEmitter();
eLC.on("x", () => {});
eLC.on("x", () => {});
const staticListenerCountType = typeof (events as any).listenerCount;

// --- EventTarget / Event / CustomEvent re-export, NOT duplicated ----------
// NOTE for the report, not a red test: real Node's `node:events` module does
// NOT actually export EventTarget/Event/CustomEvent as members at all —
// `Object.keys(require("events"))` on Node v20.19.5 has no such keys, and
// `require("events").EventTarget` is `undefined`. The doc comment in
// events/mod.rs ("Node re-exports the SAME WHATWG globals") describes
// behavior Node does not have. RTS DOES export them (a superset feature),
// and this checks RTS is at least internally consistent about it: they are
// the identical objects as the globals, not a second class.
const identityOk =
    EventTarget === (globalThis as any).EventTarget &&
    Event === (globalThis as any).Event &&
    CustomEvent === (globalThis as any).CustomEvent;

describe("node:events EventEmitter — rawListeners / off matching", () => {
    test("rawListeners()[0] is the wrapper, not the original", () => expect(rawIsNotFn).toBe(true));
    test("rawListeners()[0].listener is the original", () => expect(rawListenerIsFn).toBe(true));
    test("listeners()[0] is the original, unwrapped", () => expect(listenersIsFn).toBe(true));
    test("listenerCount before off", () => expect(countBeforeOff).toBe(1));
    test("off(original) removes the once-wrapper", () => expect(countAfterOffByOriginal).toBe(0));
    test("listenerCount before off-by-wrapper", () => expect(countBeforeOffWrap).toBe(1));
    test("off(rawListeners()[0]) also removes it", () => expect(countAfterOffByWrapper).toBe(0));
});

describe("node:events EventEmitter — prepend ordering", () => {
    test("prependListener/prependOnceListener order", () => expect(order).toBe("YZaonce-appended"));
});

describe("node:events EventEmitter — eventNames()", () => {
    test("eventNames() length with mixed keys", () => expect(namesLen).toBe(2));
    // RED on RTS: see the comment above namesOrderExpected.
    test("eventNames() orders strings before symbols (Node)", () => expect(namesOrder).toBe(namesOrderExpected));
});

describe("node:events EventEmitter — max listeners", () => {
    test("getMaxListeners default is 10", () => expect(defMax).toBe(10));
    test("setMaxListeners(25) instance", () => expect(newMax).toBe(25));
    test("module setMaxListeners(n, target) reaches the instance", () => expect(staticMaxViaInstance).toBe(50));
    test("module getMaxListeners(target) reads it back", () => expect(staticMaxViaModule).toBe(50));
    test("EventEmitter.defaultMaxListeners default is 10", () => expect(defaultBefore).toBe(10));
    test("changing defaultMaxListeners affects new instances", () => expect(maxAfterDefaultChange).toBe(5));
    // RED on RTS: `events.listenerCount` (the module-level deprecated static)
    // is missing from the namespace() member list entirely.
    test("module-level events.listenerCount exists (Node)", () => expect(staticListenerCountType).toBe("function"));
});

describe("node:events — EventTarget/Event/CustomEvent identity", () => {
    test("re-exported classes are identical to the globals", () => expect(identityOk).toBe(true));
});

// --- addAbortListener --------------------------------------------------------
// Normal case: signal not yet aborted at registration, fires once abort()
// runs — confirmed against Node, RTS agrees.
const acNormal = new AbortController();
let normalCalled = 0;
events.addAbortListener(acNormal.signal, () => { normalCalled++; });
acNormal.abort();

// BUG (confirmed against real Node): Node's addAbortListener is a plain
// `signal.addEventListener('abort', listener, {once:true})` — registering
// it AFTER the signal already aborted does NOT retroactively invoke it
// (ordinary EventTarget semantics: you cannot catch an event that already
// fired). RTS's abort.rs explicitly checks `signal.aborted` at registration
// time and calls the listener immediately when it is already true — so RTS
// answers 1 where Node answers 0. Asserting Node's answer; expected RED.
const acAlready = new AbortController();
acAlready.abort();
let alreadyAbortedCalled = 0;
events.addAbortListener(acAlready.signal, () => { alreadyAbortedCalled++; });

// invalid arguments refused with ERR_INVALID_ARG_TYPE — confirmed matching.
let badSignalCode = "";
try {
    (events as any).addAbortListener({}, () => {});
} catch (err: any) {
    badSignalCode = err.code;
}
let badListenerCode = "";
try {
    (events as any).addAbortListener(acNormal.signal, "not a function");
} catch (err: any) {
    badListenerCode = err.code;
}

describe("node:events addAbortListener", () => {
    test("fires once the signal aborts after registration", () => expect(normalCalled).toBe(1));
    // Expected RED on RTS — see the comment above acAlready.
    test("does NOT fire for a signal already aborted before registration (Node)", () => {
        expect(alreadyAbortedCalled).toBe(0);
    });
    test("invalid signal is refused ERR_INVALID_ARG_TYPE", () => expect(badSignalCode).toBe("ERR_INVALID_ARG_TYPE"));
    test("invalid listener is refused ERR_INVALID_ARG_TYPE", () => expect(badListenerCode).toBe("ERR_INVALID_ARG_TYPE"));
});
