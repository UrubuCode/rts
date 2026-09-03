// node:async_hooks — AsyncResource: runInAsyncScope, asyncId/triggerAsyncId,
// prototype.bind, the static AsyncResource.bind, and the top-level
// executionAsyncId()/executionAsyncResource() reads.
//
// AsyncLocalStorage is DELIBERATELY absent from this file: `new
// AsyncLocalStorage()` crashes the whole process (a `make_prototype`
// collision panic) before a single test() body can run — see
// tests/claude-node-async-hooks-crash.test.ts for the isolated repro and the
// root cause. Every AsyncLocalStorage scenario this task asked for (run/
// getStore/exit/enterWith/withScope, the static bind/snapshot capture-order
// trap) could not be measured against this build for that reason.
//
// Answers cross-checked against a real Node v20.19.5 via `node -e`.
import { describe, test, expect } from "rts:test";
import { AsyncResource, executionAsyncId, executionAsyncResource } from "node:async_hooks";

// ---------------------------------------------------------------------------
// Top-level reads, before any resource has entered a scope
// ---------------------------------------------------------------------------
const idOutsideAnyScope = executionAsyncId();
const resourceOutsideAnyScope = executionAsyncResource();

// ---------------------------------------------------------------------------
// Construction and its own ids
// ---------------------------------------------------------------------------
const resourceA = new AsyncResource("claude-test-A");
const resourceIdIsNumber = typeof resourceA.asyncId() === "number";
const resourceIdPositive = resourceA.asyncId() > 0;

// two resources get two DIFFERENT ids
const resourceA2 = new AsyncResource("claude-test-A2");
const distinctIds = resourceA.asyncId() !== resourceA2.asyncId();

// ---------------------------------------------------------------------------
// runInAsyncScope — id/resource visible inside, reverts after
// ---------------------------------------------------------------------------
let idInsideScope: number | undefined;
let resourceInsideScope: unknown;
resourceA.runInAsyncScope(() => {
    idInsideScope = executionAsyncId();
    resourceInsideScope = executionAsyncResource();
});
const idAfterScope = executionAsyncId();
const idMatchesInsideScope = idInsideScope === resourceA.asyncId();

// runInAsyncScope forwards thisArg and its one extra argument, and the
// return value
let capturedThis: unknown, capturedArg: unknown;
const thisTarget = { tag: "receiver" };
const scopeReturn = resourceA.runInAsyncScope(function (this: any, x: number) {
    capturedThis = this;
    capturedArg = x;
    return x * 10;
}, thisTarget, 7);

// nested scopes: the inner resource's id is visible while entered, the
// outer's is restored after
const resourceOuter = new AsyncResource("claude-outer");
const resourceInner = new AsyncResource("claude-inner");
let idNestedOuter: number | undefined, idNestedInner: number | undefined, idNestedOuterAgain: number | undefined;
resourceOuter.runInAsyncScope(() => {
    idNestedOuter = executionAsyncId();
    resourceInner.runInAsyncScope(() => {
        idNestedInner = executionAsyncId();
    });
    idNestedOuterAgain = executionAsyncId();
});

// ---------------------------------------------------------------------------
// triggerAsyncId — inherits the CURRENT scope's id when not given explicitly
// ---------------------------------------------------------------------------
let childTrigger: number | undefined;
resourceOuter.runInAsyncScope(() => {
    const child = new AsyncResource("claude-child");
    childTrigger = child.triggerAsyncId();
});
const childTriggerMatchesOuter = childTrigger === resourceOuter.asyncId();

// ---------------------------------------------------------------------------
// AsyncResource.prototype.bind — captures resource+receiver, forwards ALL
// arguments (no 4-slot limit, per the module's own doc: the resource, target
// and receiver all live in the closure environment instead of an arg slot)
// ---------------------------------------------------------------------------
const resourceB = new AsyncResource("claude-test-B");
let boundResArgs: unknown[] = [];
const resBound = resourceB.bind(function (this: any, ...args: any[]) {
    boundResArgs = args;
    return args.length;
});
const resBoundResult = resBound(1, 2, 3, 4, 5);

// default `this` for prototype.bind is the resource itself
let boundDefaultThis: unknown;
const resBoundNoThisArg = resourceB.bind(function (this: any) {
    boundDefaultThis = this;
});
resBoundNoThisArg();
const defaultThisIsResource = boundDefaultThis === resourceB;

// explicit thisArg overrides the default
let boundExplicitThis: unknown;
const explicitReceiver = { tag: "explicit" };
const resBoundWithThisArg = resourceB.bind(function (this: any) {
    boundExplicitThis = this;
}, explicitReceiver);
resBoundWithThisArg();

// ---------------------------------------------------------------------------
// AsyncResource.bind (static) — makes its own resource, delegates to bind
// ---------------------------------------------------------------------------
let staticBoundRan = false;
let staticBoundArgs: unknown[] = [];
const staticBound = (AsyncResource as any).bind(function (...args: any[]) {
    staticBoundRan = true;
    staticBoundArgs = args;
    return "ok";
});
const staticBoundResult = staticBound(1, 2);

// ---------------------------------------------------------------------------
// emitDestroy — answers `this` for chaining, and is idempotent (no throw
// from a second call, per the module's own documented trade: nothing here
// can raise a catchable JS exception)
// ---------------------------------------------------------------------------
const resourceC = new AsyncResource("claude-test-C");
const emitDestroyReturnsThis = resourceC.emitDestroy() === resourceC;
let secondEmitDestroyThrew = false;
try {
    resourceC.emitDestroy();
} catch {
    secondEmitDestroyThrew = true;
}

describe("AsyncResource — top level, before any scope", () => {
    // Real Node (v20.19.5, verified with `node -e`): the top-level script
    // itself carries async id 1 — `executionAsyncId()` is never 0 during
    // ordinary synchronous top-level code. This engine's own module doc
    // claims 0 for "outside any runInAsyncScope" as a stated, deliberate
    // divergence (no top-level resource id is minted the way Node mints
    // one) — asserted here as Node's real answer on purpose, so a mismatch
    // is visible as red rather than quietly matched.
    test("executionAsyncId() at top level is 1, Node's own script id", () => expect(idOutsideAnyScope).toBe(1));
    test("executionAsyncResource() at top level is an object, not undefined", () =>
        expect(typeof resourceOutsideAnyScope).toBe("object"));
});

describe("AsyncResource — construction", () => {
    test("asyncId() is a positive number", () => expect(resourceIdIsNumber && resourceIdPositive).toBe(true));
    test("two resources get two different ids", () => expect(distinctIds).toBe(true));
});

describe("AsyncResource — runInAsyncScope", () => {
    test("executionAsyncId() inside the scope matches the resource's own id", () =>
        expect(idMatchesInsideScope).toBe(true));
    test("executionAsyncResource() inside the scope is the resource itself", () =>
        expect(resourceInsideScope).toBe(resourceA));
    // Same documented divergence as the top-level case above: Node reverts
    // to the id active before the scope (here, 1); this engine reverts to 0.
    test("executionAsyncId() reverts to 1 (top-level) after the scope returns, not 0", () =>
        expect(idAfterScope).toBe(1));
    test("thisArg is forwarded", () => expect(capturedThis).toBe(thisTarget));
    test("the first extra argument is forwarded", () => expect(capturedArg).toBe(7));
    test("the return value is forwarded", () => expect(scopeReturn).toBe(70));
});

describe("AsyncResource — nested scopes", () => {
    test("the inner scope's id is its own resource's id", () =>
        expect(idNestedInner).toBe(resourceInner.asyncId()));
    test("the outer scope's id is restored after the inner scope returns", () =>
        expect(idNestedOuterAgain).toBe(resourceOuter.asyncId()));
    test("the outer and inner ids are different", () => expect(idNestedOuter !== idNestedInner).toBe(true));
});

describe("AsyncResource — triggerAsyncId inheritance", () => {
    test("a resource made inside a scope inherits that scope's id as its trigger", () =>
        expect(childTriggerMatchesOuter).toBe(true));
});

describe("AsyncResource.prototype.bind", () => {
    test("forwards every argument, not just the first", () =>
        expect(boundResArgs.length === 5 && boundResArgs[0] === 1 && boundResArgs[4] === 5).toBe(true));
    test("forwards the return value", () => expect(resBoundResult).toBe(5));
    test("defaults `this` to the resource itself", () => expect(defaultThisIsResource).toBe(true));
    test("an explicit thisArg overrides the default", () => expect(boundExplicitThis).toBe(explicitReceiver));
});

describe("AsyncResource.bind (static)", () => {
    test("the wrapper actually runs the target", () => expect(staticBoundRan).toBe(true));
    test("the wrapper forwards its arguments", () =>
        expect(staticBoundArgs.length === 2 && staticBoundArgs[0] === 1 && staticBoundArgs[1] === 2).toBe(true));
    test("the wrapper forwards the return value", () => expect(staticBoundResult).toBe("ok"));
});

describe("AsyncResource.prototype.emitDestroy", () => {
    test("answers `this` for chaining", () => expect(emitDestroyReturnsThis).toBe(true));
    // Verified with `node -e`: a second emitDestroy() does NOT throw in real
    // Node either — it is silently a no-op there too, so this is not a
    // divergence, just confirmed rather than assumed.
    test("a second call does not throw, matching Node", () => expect(secondEmitDestroyThrew).toBe(false));
});
