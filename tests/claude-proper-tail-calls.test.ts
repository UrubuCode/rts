// Proper tail calls (ECMAScript 2015 §15.10, strict code — this file is a
// module, so it is strict): a call in tail position runs after the caller's
// frame is gone, so its depth costs no stack.
//
// The ruler is Bun (JavaScriptCore implements PTC): every expectation below is
// what `bun` prints for the same program. Node (V8 never shipped PTC) agrees on
// every SHALLOW line and throws `RangeError: Maximum call stack size exceeded`
// on the deep ones — which is the difference the standard makes and V8 did not.
//
// The shallow lines pin what the frame being discarded must NOT take with it:
// `new.target`, `arguments`, the receiver, a throw, a `finally` that runs AFTER
// the call it guards, and a condition read once.
import { describe, test, expect } from "rts:test";

// ---- depth: none of these fits in any stack without the tail call --------
function loop(n: number): string {
  if (n <= 0) return "done";
  return loop(n - 1);
}
function sumAcc(n: number, acc: number): number {
  return n <= 0 ? acc : sumAcc(n - 1, acc + n);
}
// A mutual pair small enough to be SUBSTITUTED into each other: `odd` is
// emitted inside `even`, so the substituted body's own `return even(...)` has
// to stay a tail call or `even` recurses into itself for real.
function even(n: number): boolean {
  if (n === 0) return true;
  return odd(n - 1);
}
function odd(n: number): boolean {
  if (n === 0) return false;
  return even(n - 1);
}
const obj = {
  k: 7,
  down(n: number): number {
    return n <= 0 ? this.k : this.down(n - 1);
  },
};

// ---- what the discarded frame must not take with it ------------------------
const seen: string[] = [];
function reportsTarget(): number {
  seen.push(new.target === undefined ? "called" : "constructed");
  return 1;
}
function Ctor(this: any) {
  return reportsTarget();
}
const constructed = new (Ctor as any)();

function countArgs(..._xs: unknown[]): number {
  return arguments.length;
}
function five(a: number, b: number, c: number, d: number, e: number): number {
  return countArgs(a, b);
}

function notCallable(): unknown {
  const x: any = 5;
  return x();
}
let caught = "";
try {
  notCallable();
} catch (e) {
  caught = (e as Error).constructor.name;
}

class Klass {}
function callsClass(): unknown {
  return (Klass as any)();
}
let classCaught = "";
try {
  callsClass();
} catch (e) {
  classCaught = (e as Error).constructor.name;
}

function thrower(): never {
  throw new Error("boom");
}
function throwsThroughTail(): unknown {
  return thrower();
}
let thrown = "";
try {
  throwsThroughTail();
} catch (e) {
  thrown = (e as Error).message;
}

const order: string[] = [];
function note(x: number): number {
  order.push("call");
  return x;
}
function guarded(x: number): number {
  try {
    return note(x);
  } finally {
    order.push("finally");
  }
}
const guardedAnswer = guarded(3);

let reads = 0;
const flag = {
  get on(): boolean {
    reads++;
    return true;
  },
};
function pick(n: number): string {
  return flag.on ? (n > 0 ? pick(n - 1) : "bottom") : "never";
}
const picked = pick(3);

function Obj(this: any) {
  return makeObject();
}
function makeObject() {
  return { made: "by the tail callee" };
}

describe("proper tail calls — depth", () => {
  test("self recursion, a million levels", () => expect(loop(1000000)).toBe("done"));
  test("an accumulator written as a conditional", () =>
    expect(sumAcc(1000000, 0)).toBe(500000500000));
  test("a mutual pair the inliner substitutes into each other", () =>
    expect(odd(1000001)).toBe(true));
  test("a method calling itself through `this`", () => expect(obj.down(1000000)).toBe(7));
});

describe("proper tail calls — what the discarded frame must not take", () => {
  test("a function reached by a constructor's tail call is CALLED", () =>
    expect(seen.join(",")).toBe("called"));
  test("`new` of a tail-calling constructor still answers its object", () =>
    expect(typeof constructed).toBe("object"));
  test("a constructor's tail call answering an object is the result", () =>
    expect(new (Obj as any)().made).toBe("by the tail callee"));
  test("the callee counts its own arguments", () => expect(five(1, 2, 3, 4, 5)).toBe(2));
  test("calling a non-function in tail position is a catchable TypeError", () =>
    expect(caught).toBe("TypeError"));
  test("a class called without `new` in tail position is still a TypeError", () =>
    expect(classCaught).toBe("TypeError"));
  test("a throw from a tail callee reaches the caller's catch", () =>
    expect(thrown).toBe("boom"));
  test("inside `try`/`finally` the call runs BEFORE the finally", () =>
    expect(order.join(",") + "=" + guardedAnswer).toBe("call,finally=3"));
  test("the condition is read once per level", () =>
    expect(picked + " " + reads).toBe("bottom 4"));
});
