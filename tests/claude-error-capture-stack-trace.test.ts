// `Error.captureStackTrace` and `Error.stackTraceLimit` — V8's stack trace API,
// which Node exposes and half the npm ecosystem calls. `node_modules/ws` calls
// `Error.captureStackTrace(err, abortHandshake)`; it was `undefined` here, so a
// refused WebSocket handshake died on `undefined is not a function`.
//
// What is pinned here is the CONTRACT measured against Node 22.23.2, not the
// contents of the trace. Node's frames carry `(file.js:7:40)` and this engine's
// carry only `at <name>` — nothing maps a code address back to a source position
// at run time (issue #2862) — so every assertion below is about the shape of the
// property and which frames are in it, never about a line number.

import { describe, test, expect } from "rts:test";

const results: string[] = [];
function check(name: string, ok: boolean): void {
  results.push((ok ? "ok   " : "FAIL ") + name);
}

// --- it exists, and with Node's identity ------------------------------------

check("captureStackTrace is a function", typeof Error.captureStackTrace === "function");
check("its length is 2", (Error.captureStackTrace as any).length === 2);
check("its name is captureStackTrace", (Error.captureStackTrace as any).name === "captureStackTrace");

const own = Object.getOwnPropertyDescriptor(Error, "captureStackTrace") as any;
check("it is an own property of Error", own !== undefined);
check("it is not enumerable", own !== undefined && own.enumerable === false);
check("it is configurable", own !== undefined && own.configurable === true);
check("it is writable", own !== undefined && own.writable === true);

// --- a plain object gets a string -------------------------------------------

function inner(): any {
  const target: any = {};
  Error.captureStackTrace(target);
  return target;
}
// NOT a one-line forwarder, and that is load-bearing. `function outer() { return
// inner(); }` is inlined by this engine's emitter, so it contributes no frame and
// the assertion below would have measured the inliner instead of the capture.
// `new Error().stack` loses the same frame, so the shortfall belongs to the trace
// and not to this entry — written up at the foot of this file.
function outer(): any {
  let keep = 0;
  for (let i = 0; i < 3; i++) {
    keep += i;
  }
  const got = inner();
  if (keep === 999) {
    console.log("never");
  }
  return got;
}
const captured = outer();
check("a plain object gets a string stack", typeof captured.stack === "string");
check("the header of a bare object is Error", captured.stack.split("\n")[0] === "Error");
check("stack is not enumerable", Object.keys(captured).length === 0);
check("the capture names its caller", captured.stack.indexOf("at inner") >= 0);
check("and its caller's caller", captured.stack.indexOf("at outer") >= 0);
// The native must not name ITSELF: Node's trace opens at the function that
// called captureStackTrace.
check(
  "the native's own frame is absent",
  captured.stack.indexOf("captureStackTrace") < 0,
);

// --- the header comes from the target's own name and message ----------------

const labelled: any = { name: "Foo", message: "bar" };
Error.captureStackTrace(labelled);
check("the header is `name: message`", labelled.stack.split("\n")[0] === "Foo: bar");

// --- the second argument hides a frame and everything inside it -------------

function helper(): any {
  const target: any = {};
  Error.captureStackTrace(target, helper);
  return target;
}
function caller(): any {
  let keep = 0;
  for (let i = 0; i < 3; i++) {
    keep += i;
  }
  const got = helper();
  if (keep === 999) {
    console.log("never");
  }
  return got;
}
const hidden = caller();
check("the named frame is gone", hidden.stack.indexOf("at helper") < 0);
check("its caller is kept", hidden.stack.indexOf("at caller") >= 0);

// A function that is not on the stack drops every frame — V8 skips until it
// finds the function and never stops skipping when it is not there.
function elsewhere(): void {}
function absent(): any {
  const target: any = {};
  Error.captureStackTrace(target, elsewhere);
  return target;
}
check("a function not on the stack leaves the header alone", absent().stack === "Error");

// A non-callable second argument is ignored, not an error.
let ignored = true;
try {
  const target: any = {};
  Error.captureStackTrace(target, 5 as any);
  ignored = typeof target.stack === "string";
} catch (_e) {
  ignored = false;
}
check("a non-callable second argument is ignored", ignored);

// --- a non-object first argument throws ------------------------------------

for (const bad of [undefined, null, 5, "x", true]) {
  let threw = "";
  try {
    (Error.captureStackTrace as any)(bad);
  } catch (e) {
    threw = (e as any) instanceof TypeError ? "TypeError" : "other";
  }
  check("captureStackTrace(" + String(bad) + ") is a TypeError", threw === "TypeError");
}
let noArgs = "";
try {
  (Error.captureStackTrace as any)();
} catch (e) {
  noArgs = (e as any) instanceof TypeError ? "TypeError" : "other";
}
check("captureStackTrace() with no argument is a TypeError", noArgs === "TypeError");

// --- an Error passed to it has its deferred trace replaced ------------------
//
// This is the `ws` call. The constructor already captured frames lazily, so the
// capture has to drop them: otherwise the inherited accessor would overwrite
// this answer the first time something read `.stack`.

function raise(): Error {
  const err = new Error("refused");
  Error.captureStackTrace(err, raise);
  return err;
}
function around(): Error {
  let keep = 0;
  for (let i = 0; i < 3; i++) {
    keep += i;
  }
  const got = raise();
  if (keep === 999) {
    console.log("never");
  }
  return got;
}
const err = around();
check("an Error keeps its header", err.stack!.split("\n")[0] === "Error: refused");
check("the named frame is gone from an Error too", err.stack!.indexOf("at raise") < 0);
check("the outer frame survives", err.stack!.indexOf("at around") >= 0);
check("a second read answers the same text", err.stack === err.stack);

// --- Error.stackTraceLimit --------------------------------------------------

const limit = Object.getOwnPropertyDescriptor(Error, "stackTraceLimit") as any;
check("stackTraceLimit defaults to 10", Error.stackTraceLimit === 10);
check("stackTraceLimit is writable", limit !== undefined && limit.writable === true);
check("stackTraceLimit is enumerable", limit !== undefined && limit.enumerable === true);
check("stackTraceLimit is configurable", limit !== undefined && limit.configurable === true);

Error.stackTraceLimit = 0;
function none(): any {
  const target: any = {};
  Error.captureStackTrace(target);
  return target;
}
check("a limit of 0 answers the header alone", none().stack === "Error");

Error.stackTraceLimit = 1;
function d1(): any {
  const target: any = {};
  Error.captureStackTrace(target);
  return target;
}
function d2(): any {
  return d1();
}
function d3(): any {
  return d2();
}
const one = d3().stack as string;
check("a limit of 1 keeps one frame", one.split("\n").length === 2);
check("and it is the innermost one", one.indexOf("at d1") >= 0);

Error.stackTraceLimit = 10;

// --- the divergences, written down -----------------------------------------
//
// Measured in Node 22 and NOT matched here, each for a stated reason:
//
//  * Node's frames carry `(file:line:column)`; these carry the name alone. The
//    machine records a source position per instruction and nothing maps an
//    address back to one at run time (issue #2862).
//  * An INLINED frame is absent. `function outer() { return inner(); }` is
//    inlined, so neither `outer` nor — when it is itself thin — `inner` appears,
//    where Node names both. Measured both ways: `new Error("boom").stack` loses
//    exactly the same frame, so this is the trace's shortfall and not this
//    entry's, and the three cases above use non-inlinable wrappers so that what
//    they measure is the capture.
//  * Node installs `stack` as an accessor PAIR and recomputes the header on the
//    first read, so assigning `obj.name` after the capture changes the header
//    there. Here the string is rendered at the capture. The deferral table this
//    engine already has could not be reused: its presence is what
//    `Object.prototype.toString` reads to decide a cell is an Error, so
//    deferring on a plain object would answer `[object Error]` for it.
//  * `Error.captureStackTrace(Object.freeze({}))` is a TypeError in Node
//    ("Cannot define property stack, object is not extensible"); here the write
//    is simply dropped.
//  * `Error.stackTraceLimit = "abc"` makes Node omit `.stack` entirely; here a
//    non-number reads as 0, so the header survives. A `.stack` of `undefined` is
//    the failure this surface exists to remove.
//  * `Error.prepareStackTrace` is still absent. It hands the hook an array of
//    CallSite objects with 23 methods, most of which answer a position this
//    engine cannot produce, so it would be a surface that does not do what its
//    name says. Left out deliberately, not overlooked.
check(
  "a frozen target is a known divergence, not a crash",
  (() => {
    try {
      Error.captureStackTrace(Object.freeze({}));
      return true;
    } catch (_e) {
      return true;
    }
  })(),
);

// One `test` per line, so a failure names the behaviour that broke rather than a
// count. Registered in a loop over what the body above already measured: the
// capture has to happen at a known call depth, which a closure inside `test`
// would change.
describe("Error.captureStackTrace", () => {
  for (const line of results) {
    const named = line.slice(5);
    const passed = line.indexOf("ok") === 0;
    test(named, () => expect(passed).toBe(true));
  }
});

let failed = 0;
for (const line of results) {
  if (line.indexOf("FAIL") === 0) {
    failed++;
  }
  console.log(line);
}
console.log((results.length - failed) + " ok, " + failed + " fail");
