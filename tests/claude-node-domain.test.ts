// node:domain — create/createDomain/Domain, run/enter/exit, add/remove
// bookkeeping, bind, and intercept's no-error branch. Deliberately excludes
// every scenario that reaches `domain.emit('error', …)` with the domain
// itself as the emit target — see claude-node-domain-crash.test.ts: that
// path kills the process in this build, root-caused there. Answers
// cross-checked against a real Node v20.19.5 via `node -e`.
import { describe, test, expect } from "rts:test";
import domain from "node:domain";

const EventEmitter: any = require("events");

// ---------------------------------------------------------------------------
// create() / createDomain() / new Domain()
// ---------------------------------------------------------------------------
const d1 = domain.create();
const d1IsObject = typeof d1 === "object" && d1 !== null;
const d1MembersEmpty = Array.isArray((d1 as any).members) && (d1 as any).members.length === 0;

const d2 = (domain as any).createDomain();
const d2IsObject = typeof d2 === "object" && d2 !== null;

// Node: `createDomain` and `create` are the SAME function reference
// (`exports.createDomain = exports.create = function create(...)`).
// Verified with `node -e`.
const createDomainIsCreate = (domain as any).createDomain === domain.create;

const d3 = new (domain as any).Domain();
const d3InstanceOf = d3 instanceof (domain as any).Domain;
const d3MembersEmpty = Array.isArray(d3.members) && d3.members.length === 0;

// ---------------------------------------------------------------------------
// add() / remove() bookkeeping — the array, and ownership transfer
// ---------------------------------------------------------------------------
const dA = domain.create();
const dB = domain.create();
const emitterAB = new EventEmitter();
dA.add(emitterAB);
const inAAfterAdd = dA.members.length === 1 && dA.members[0] === emitterAB;

// re-adding the SAME emitter to a second domain moves it — it belongs to at
// most one domain — rather than stacking membership.
dB.add(emitterAB);
const goneFromAAfterReAdd = dA.members.length === 0;
const inBAfterReAdd = dB.members.length === 1 && dB.members[0] === emitterAB;

// remove() clears membership
dB.remove(emitterAB);
const emptyAfterRemove = dB.members.length === 0;

// An emitter added to a domain still delivers an ORDINARY (non-'error')
// event to its own listener — the override installed by add() must be a
// transparent passthrough for anything that is not 'error'. Tested both
// while still a member and after remove(), since the module's own doc says
// remove() leaves the installed override in place rather than restoring the
// original `emit`.
const dC = domain.create();
const emitterC = new EventEmitter();
dC.add(emitterC);
let dataWhileMember = false;
emitterC.on("data", () => {
    dataWhileMember = true;
});
emitterC.emit("data", 1);

dC.remove(emitterC);
let dataAfterRemove = false;
emitterC.on("more-data", () => {
    dataAfterRemove = true;
});
emitterC.emit("more-data", 1);

// ---------------------------------------------------------------------------
// run() — push/call/pop, return value, nesting
// ---------------------------------------------------------------------------
const dRun = domain.create();
let ranInside = false;
const runReturn = dRun.run(() => {
    ranInside = true;
    return 99;
});

const dOuter = domain.create();
const dInner = domain.create();
let nestedRanBoth = false;
dOuter.run(() => {
    dInner.run(() => {
        nestedRanBoth = true;
    });
});

// A throw INSIDE run()'s callback propagates as an ordinary catchable JS
// exception — real Node behaves identically here (verified with `node -e`):
// domain.run() does not itself try/catch synchronously, so this is NOT a
// divergence, just confirmed. What Node's domain actually adds is routing an
// otherwise-UNCAUGHT exception via process-level hooks, which is out of
// scope for this module (see its own doc: "process.domain" is unbuilt).
const dThrow = domain.create();
let caughtOutsideRun = false;
let throwMessage = "";
try {
    dThrow.run(() => {
        throw new Error("inside run");
    });
} catch (e: any) {
    caughtOutsideRun = true;
    throwMessage = e && e.message;
}
// the domain is still usable after an escaped throw
const dThrowStillUsable = dThrow.run(() => 7) === 7;

// ---------------------------------------------------------------------------
// enter() / exit() — smoke-tested for "does not throw" only: their real
// effect is supposed to be observable through `domain.active`, which this
// build never updates (see the `domain.active` block below) — so there is
// no OTHER way to observe enter/exit succeeding from JS in this build.
// ---------------------------------------------------------------------------
const dEnter = domain.create();
let enterExitThrew = false;
try {
    dEnter.enter();
    dEnter.exit();
    dEnter.exit(); // a second exit() with nothing matching is documented as a no-op
} catch {
    enterExitThrew = true;
}

// ---------------------------------------------------------------------------
// domain.active
// ---------------------------------------------------------------------------
// Real Node (verified with `node -e`, v20.19.5):
//   domain.active is `null` before anything runs.
//   domain.create() alone does NOT change it (still `null`).
//   Inside d.run(fn), domain.active === d is `true`.
//   After run() returns, domain.active is `undefined` (not restored to null).
// This engine's `refresh_active` is called ONLY from `fresh()` (i.e. from
// create()/new Domain()) and never from run()/enter()/exit() — so
// `domain.active` here is frozen at whatever it read the last time a domain
// object was constructed, and never reflects what is actually entered.
const activeBeforeAnything = (domain as any).active;
const dActive = domain.create();
const activeAfterCreate = (domain as any).active;
let activeInsideRun: unknown;
dActive.run(() => {
    activeInsideRun = (domain as any).active;
});
const activeAfterRunReturns = (domain as any).active;

// ---------------------------------------------------------------------------
// bind() — forwards args/return, and a throw inside is catchable and
// leaves the domain itself still usable afterward
// ---------------------------------------------------------------------------
const dBind = domain.create();
const boundAdd = dBind.bind((a: number, b: number) => a + b);
const boundAddResult = boundAdd(3, 4);

const boundThrows = dBind.bind(() => {
    throw new Error("from bound callback");
});
let boundThrowCaught = false;
try {
    boundThrows();
} catch {
    boundThrowCaught = true;
}
const dBindStillUsableAfterThrow = dBind.bind((x: number) => x * 2)(21);

// ---------------------------------------------------------------------------
// intercept() — the no-error branch only (the error branch reaches the same
// broken domain.emit() the crash file isolates; see that file).
// ---------------------------------------------------------------------------
const dIntercept = domain.create();
const doubler = dIntercept.intercept((val: number) => val * 2);
const interceptNoErrResult = doubler(null, 21);

describe("domain — create / createDomain / Domain", () => {
    test("create() answers an object with an empty members array", () =>
        expect(d1IsObject && d1MembersEmpty).toBe(true));
    test("createDomain() answers an object too", () => expect(d2IsObject).toBe(true));
    test("createDomain === create (same function, per real Node)", () => expect(createDomainIsCreate).toBe(true));
    test("new domain.Domain() answers a real instance", () => expect(d3InstanceOf).toBe(true));
    test("new domain.Domain() also starts with empty members", () => expect(d3MembersEmpty).toBe(true));
});

describe("domain — add() / remove() bookkeeping", () => {
    test("add() records the emitter", () => expect(inAAfterAdd).toBe(true));
    test("re-adding to a second domain removes it from the first", () => expect(goneFromAAfterReAdd).toBe(true));
    test("re-adding to a second domain records it there", () => expect(inBAfterReAdd).toBe(true));
    test("remove() clears membership", () => expect(emptyAfterRemove).toBe(true));
    test("a non-'error' event still reaches the emitter's own listener while a member", () =>
        expect(dataWhileMember).toBe(true));
    test("a non-'error' event still reaches the emitter's own listener after remove()", () =>
        expect(dataAfterRemove).toBe(true));
});

describe("domain — run()", () => {
    test("the callback actually runs", () => expect(ranInside).toBe(true));
    test("the return value is forwarded", () => expect(runReturn).toBe(99));
    test("nested run() from two different domains both execute", () => expect(nestedRanBoth).toBe(true));
    test("a throw inside run() propagates as an ordinary catchable exception (matches Node)", () =>
        expect(caughtOutsideRun && throwMessage === "inside run").toBe(true));
    test("the domain is still usable after an escaped throw", () => expect(dThrowStillUsable).toBe(true));
});

describe("domain — enter() / exit()", () => {
    test("enter()/exit()/a redundant exit() do not throw", () => expect(enterExitThrew).toBe(false));
});

describe("domain.active — asserted as real Node answers it (v20.19.5)", () => {
    test("is null before anything runs (Node), not undefined", () => expect(activeBeforeAnything).toBe(null));
    test("create() alone does not change it — still null", () => expect(activeAfterCreate).toBe(null));
    test("inside run(), active is the running domain", () => expect(activeInsideRun).toBe(dActive));
    test("after run() returns, active is undefined again", () => expect(activeAfterRunReturns).toBe(undefined));
});

describe("domain — bind()", () => {
    test("forwards arguments and the return value", () => expect(boundAddResult).toBe(7));
    test("a throw inside the wrapped callback is an ordinary catchable exception", () =>
        expect(boundThrowCaught).toBe(true));
    test("the domain is still usable for a fresh bind() after a throw", () =>
        expect(dBindStillUsableAfterThrow).toBe(42));
});

describe("domain — intercept(), no-error branch", () => {
    test("a null first argument calls the wrapped callback with the rest", () =>
        expect(interceptNoErrResult).toBe(42));
});
