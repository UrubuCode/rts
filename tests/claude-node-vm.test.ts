// node:vm — `entry::evaluate`'s isolated program vs. `evaluate_in_scope_with_receiver`'s
// same-region compile. `crates/rts-node/src/vm.rs`'s own doc names the limit up
// front: only a value that needs no region (number/boolean/singleton) crosses
// `entry::evaluate`'s boundary, and an object (including a function) reads back
// `undefined` there. `tests/claude-page-scope-declara.test.ts` already exercises
// var/function/let leakage through `runInContext` with console.log (no
// assertions) — this file does not repeat that ground, and instead covers the
// rest of the surface with real pass/fail.
import { describe, test, expect } from "rts:test";
import * as vm from "node:vm";

// ── vm.runInNewContext / runInContext / runInThisContext: arithmetic ────────
const a1 = vm.runInNewContext("40 + 2");
const a2 = vm.runInContext("40 + 2", vm.createContext({}));
const a3 = vm.runInThisContext("40 + 2");

// ── vm.createContext / vm.isContext ──────────────────────────────────────────
const ctxObj: any = { seed: 1 };
const returnedCtx = vm.createContext(ctxObj);
const createContextReturnsSameRef = returnedCtx === ctxObj;
const isContextTrueAfterCreate = vm.isContext(ctxObj);
const isContextFalseForPlainObject = vm.isContext({});

// ── vm.createContext() with no argument builds one ──────────────────────────
const impliedCtx = vm.createContext();
const impliedCtxIsObject = typeof impliedCtx === "object" && impliedCtx !== null;
const impliedCtxIsContext = vm.isContext(impliedCtx);

// ── new vm.Script + its three run methods ────────────────────────────────────
const script = new vm.Script("1 + 1");
const scriptIsObject = typeof script === "object";
const scriptRunThis = script.runInThisContext();
const scriptRunNew = script.runInNewContext();
const scriptCtx: any = {};
vm.createContext(scriptCtx);
const scriptRunCtx = script.runInContext(scriptCtx);

// ── a context object supplies `this` and free-name lookup ───────────────────
const thisCtx: any = { greeting: "hi" };
vm.createContext(thisCtx);
const readsFreeNameFromContext = vm.runInContext("greeting", thisCtx);
const thisIsTheContextObject = vm.runInContext("this === globalThis ? 'global' : (this && this.greeting)", thisCtx);

// ── a var/function declared in one runInContext call is visible to the next
// ON THE SAME context object (the mechanism a bundle depends on) ────────────
const persistCtx: any = {};
vm.createContext(persistCtx);
vm.runInContext("var counter = 10;", persistCtx);
const secondCallSeesVar = vm.runInContext("counter", persistCtx);

// ── DOES an object/array completion value cross runInContext? ───────────────
// The module doc's "THE limit" section is about `entry::evaluate`'s isolated
// program; runInContext/runInNewContext/runInThisContext instead go through
// `evaluate_in_scope_with_receiver`, which the doc says keeps a completion
// value USABLE — so this is read from the engine's actual answer, not assumed.
const objCompletionNewContext = vm.runInNewContext("({ a: 1, b: 2 })");
const objCompletionNewContextOk =
    objCompletionNewContext != null &&
    typeof objCompletionNewContext === "object" &&
    (objCompletionNewContext as any).a === 1;

const arrCompletionThisContext = vm.runInThisContext("[1, 2, 3]");
const arrCompletionOk = Array.isArray(arrCompletionThisContext) && arrCompletionThisContext.length === 3;

// ── vm.compileFunction: primitive params — BUG, not the documented limit ────
// `compile_function`'s own doc says a number/string/boolean argument SHOULD
// bind by being spliced in as a source literal. It does not: `param_names`
// reads each element of the `params` array with
// `entry::get_member(context, params, &index.to_string())` — a STRING key
// ("0", "1", …) built at runtime — and that answers `undefined` for every
// index on a real array, where `entry::get_indexed(params, make_number(i))`
// (what `wasi/mod.rs`'s own `read_string_array` uses for the identical job)
// would not. So `names` is always empty, `__params__` is stored as `""`, and
// the wrapper becomes `(function() { <body> })()` — a zero-parameter call
// whose body still references the free names `a`/`b`. Isolated in
// `claude-node-vm-crash.test.ts` is what that does when the body's
// ReferenceError is not silently swallowed. Verified directly:
// `(addFn as any).__params__` reads back `""` right after construction, with
// `(addFn as any).__body__` correctly `"return a + b;"` beside it — so the
// body crosses and the params list alone is lost.
const addFn = vm.compileFunction("return a + b;", ["a", "b"]);
const addFnIsFunction = typeof addFn === "function";
const addFnResult = addFn(3, 4);
const addFnParamsStoredEmpty = (addFn as any).__params__ === "";

const concatFn = vm.compileFunction("return a + '-' + b;", ["a", "b"]);
const concatResult = concatFn("x", "y");

// ── DIVERGENCE (documented): compileFunction's callable re-runs the source
// through a fresh, isolated `entry::evaluate` program EVERY CALL, splicing
// each argument in as a source literal. Only a number/string/boolean can be
// spliced that way — an object argument is passed through as `undefined`
// rather than binding the real value. Real Node's compileFunction runs in the
// live realm and an object argument binds normally. (Moot in practice right
// now, since NO argument binds at all per the bug above — but this is the
// separate, INTENDED limit for the day the params bug is fixed, so it is
// tested with a zero-parameter function whose body reaches no free name.)
const objReturnFn = vm.compileFunction("return { made: 'inside' };", []);
const objReturnResult = objReturnFn();

// ── vm.constants exists (accepted, mostly inert per the module doc) ─────────
const constantsExists = typeof (vm as any).constants === "object" || typeof (vm as any).constants === "undefined";

// ── options objects are accepted without throwing (ignored, per doc) ────────
let timeoutOptionThrew = false;
try {
    vm.runInNewContext("1+1", {}, { timeout: 50 });
} catch (e) {
    timeoutOptionThrew = true;
}

describe("node:vm — plain evaluation", () => {
    test("runInNewContext computes arithmetic", () => expect(a1).toBe(42));
    test("runInContext computes arithmetic", () => expect(a2).toBe(42));
    test("runInThisContext computes arithmetic", () => expect(a3).toBe(42));
});

describe("node:vm — createContext / isContext", () => {
    test("createContext(obj) returns the SAME object", () => expect(createContextReturnsSameRef).toBe(true));
    test("isContext is true right after createContext", () => expect(isContextTrueAfterCreate).toBe(true));
    test("isContext is false for an unmarked object", () => expect(isContextFalseForPlainObject).toBe(false));
    test("createContext() with no arg builds an object", () => expect(impliedCtxIsObject).toBe(true));
    test("...and that object is itself a context", () => expect(impliedCtxIsContext).toBe(true));
});

describe("node:vm — Script", () => {
    test("new vm.Script(code) constructs", () => expect(scriptIsObject).toBe(true));
    test("script.runInThisContext() evaluates", () => expect(scriptRunThis).toBe(2));
    test("script.runInNewContext() evaluates", () => expect(scriptRunNew).toBe(2));
    test("script.runInContext(ctx) evaluates", () => expect(scriptRunCtx).toBe(2));
});

describe("node:vm — context supplies free names and `this`", () => {
    test("a free name resolves against the context object", () => expect(readsFreeNameFromContext).toBe("hi"));
    test("`this` inside the evaluated code is the context object", () => expect(thisIsTheContextObject).toBe("hi"));
    test("a var from one call is visible to the next, same context", () => expect(secondCallSeesVar).toBe(10));
});

describe("node:vm — object/array completion values DO cross runInContext-family calls", () => {
    test("an object literal completion crosses runInNewContext", () => expect(objCompletionNewContextOk).toBe(true));
    test("an array literal completion crosses runInThisContext", () => expect(arrCompletionOk).toBe(true));
});

describe("node:vm — compileFunction, primitive args (RED: real bug, see comment above)", () => {
    test("compileFunction returns a real callable", () => expect(addFnIsFunction).toBe(true));
    test("__params__ is stored empty (root cause, not Node's behaviour)", () => expect(addFnParamsStoredEmpty).toBe(true));
    test("numeric args bind and compute — Node answers 7, this engine cannot", () => expect(addFnResult).toBe(7));
    test("string args bind and compute — Node answers 'x-y', this engine cannot", () => expect(concatResult).toBe("x-y"));
});

describe("node:vm — compileFunction, object return value (documented limit, NOT a bug)", () => {
    // Real Node: objReturnResult.made === 'inside'. This engine's compiled
    // callable re-runs through `entry::evaluate` on every call, whose own
    // limit is that only a value needing no region crosses back — an object
    // reads as `undefined` here instead.
    test("an object return value crosses back intact", () => expect(objReturnResult && (objReturnResult as any).made).toBe("inside"));
});

describe("node:vm — options objects are accepted, not type errors", () => {
    test("an unsupported option (timeout) does not throw", () => expect(timeoutOptionThrew).toBe(false));
});
