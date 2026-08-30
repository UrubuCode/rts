// What `emit/inline.rs` may substitute, now that it takes a STATEMENT body and
// admits FREE names.
//
// Every case here is a way the substitution can be silently wrong, and two of
// them were: a body-local `const` wrote the CALLER's binding of the same name,
// and a named function expression's own name was admitted as free and then
// resolved against a caller that does not declare it. Both shipped in a build
// and were caught by this file before it reached anything else.
//
// The assertions are about VALUES, never about whether a call was inlined: a
// refusal is always correct and always slower, so a test that asserted the
// substitution happened would fail for the safe reason.
import { describe, test, expect } from "rts:test";

describe("a statement body substituted at its call site", () => {
  // The shape this was extended for: statements, an `if`, and a free name the
  // body both reads and WRITES.
  let state = 1;
  function step(): number {
    state = (state * 1664525 + 1013904223) % 4294967296;
    if (state < 0) state = state + 4294967296;
    return state / 4294967296;
  }

  test("a written free name advances, and the writes are visible outside", () => {
    state = 1;
    const first = step();
    const second = step();
    expect(first).toBe(1015568748 / 4294967296);
    expect(second === first).toBe(false);
    expect(state > 0 && state < 4294967296).toBe(true);
  });

  test("the same calls in a loop produce the same sequence", () => {
    state = 1;
    const straight = [step(), step(), step()];
    state = 1;
    const looped: number[] = [];
    for (let i = 0; i < 3; i++) looped.push(step());
    expect(looped.join(",")).toBe(straight.join(","));
  });

  test("an argument is evaluated once, where it was written", () => {
    const log: string[] = [];
    let counter = 0;
    function bump(tag: string): number {
      counter = counter + 1;
      log.push(tag + counter);
      return counter;
    }
    function twice(x: number): number {
      return x + x;
    }
    expect(twice(bump("a"))).toBe(2);
    expect(log.join(",")).toBe("a1");
    expect(counter).toBe(1);
  });

  test("a body local does NOT write the caller's binding of that name", () => {
    // `t` exists in both, and the two are different bindings. On the build that
    // admitted a declaring body, `localsOnly(3)` set the caller's `t` to 6.
    function localsOnly(n: number): number {
      const t = n * 2;
      return t + 1;
    }
    const t = 500;
    expect(localsOnly(3)).toBe(7);
    expect(t).toBe(500);
  });

  test("a name declared twice in the program is not admitted as free", () => {
    let ambiguous = 10;
    function reads(): number {
      return ambiguous;
    }
    function shadows(): number {
      const ambiguous = 20;
      return ambiguous + reads();
    }
    expect(shadows()).toBe(30);
    expect(reads()).toBe(10);
  });

  test("a named function expression keeps its own name", () => {
    // The name is bound INSIDE the body and nowhere else, so a body that uses
    // it cannot be spliced into a caller. `ReferenceError: fact is not defined`
    // on the build that admitted it.
    const selfNamed = function fact(n: number): number {
      if (n <= 1) {
        return 1;
      }
      return n * fact(n - 1);
    };
    expect(selfNamed(5)).toBe(120);
    const tail = function loop(n: number): number {
      if (n > 0) {
        return loop(n - 1);
      }
      return 7;
    };
    expect(tail(3)).toBe(7);
  });

  test("an if with an else, writing a free name in both branches", () => {
    let sign = 0;
    function classify(n: number): number {
      if (n < 0) {
        sign = -1;
      } else {
        sign = 1;
      }
      return sign * n;
    }
    expect(classify(-5)).toBe(5);
    expect(sign).toBe(-1);
    expect(classify(7)).toBe(7);
    expect(sign).toBe(1);
  });

  test("recursion is not substituted", () => {
    function fact(n: number): number {
      if (n <= 1) {
        return 1;
      }
      return n * fact(n - 1);
    }
    expect(fact(5)).toBe(120);
  });

  test("a free name that is a function is called, not substituted away", () => {
    function helper(n: number): number {
      return n * 10;
    }
    function usesHelper(n: number): number {
      return helper(n) + 1;
    }
    expect(usesHelper(4)).toBe(41);
  });

  test("the function is still a value with a name and a type", () => {
    expect(typeof step).toBe("function");
    expect(step.name).toBe("step");
    const held: () => number = step;
    state = 1;
    expect(held()).toBe(1015568748 / 4294967296);
  });

  test("a wrong arity falls back to a real call", () => {
    function two(a: number, b: number): number {
      return a + b;
    }
    expect(two(1, 2)).toBe(3);
    expect(Number.isNaN((two as any)(1))).toBe(true);
  });


  test("a helper declared inside a function is substituted there", () => {
    function run(n: number): number {
      function nested(x: number): number {
        return x + 1;
      }
      let a = 0;
      for (let i = 0; i < n; i++) a = nested(a);
      return a;
    }
    expect(run(5)).toBe(5);
  });

  test("a const arrow declared inside a function is substituted there", () => {
    function run(n: number): number {
      const nested = (x: number): number => x * 2;
      let a = 1;
      for (let i = 0; i < n; i++) a = nested(a);
      return a;
    }
    expect(run(4)).toBe(16);
  });

  test("a nested helper with statements and a free name", () => {
    let seen = 0;
    function run(n: number): number {
      function step(x: number): number {
        seen = seen + 1;
        if (seen > 1000) seen = 0;
        return x + 2;
      }
      let a = 0;
      for (let i = 0; i < n; i++) a = step(a);
      return a;
    }
    expect(run(3)).toBe(6);
    expect(seen).toBe(3);
  });

  test("a name bound only inside another function does NOT leak to a sibling", () => {
    // The hazard the scope gate closes. `helper` is declared exactly once in
    // the program, so the declaration count cannot tell these two sites apart;
    // only the scope chain can. In `sibling` the name is not bound at all, so
    // the call must reach whatever `helper` means there — which is nothing, and
    // the program says so rather than quietly running the other body.
    function owner(): number {
      function helper(x: number): number {
        return x + 100;
      }
      return helper(1);
    }
    expect(owner()).toBe(101);
    let reached = "";
    try {
      // eslint-disable-next-line
      const sibling = new Function("return typeof helper");
      reached = String(sibling());
    } catch {
      reached = "refused";
    }
    expect(reached === "undefined" || reached === "refused").toBe(true);
  });

  test("two helpers of the same name in two functions are both refused", () => {
    // Declared twice in the program, so `declarations_of` is 2 and neither is a
    // candidate — the values still have to be right.
    function first(): number {
      function same(x: number): number {
        return x + 1;
      }
      return same(10);
    }
    function second(): number {
      function same(x: number): number {
        return x + 2;
      }
      return same(10);
    }
    expect(first()).toBe(11);
    expect(second()).toBe(12);
  });

  test("a nested helper closing over the enclosing function's local", () => {
    // `base` is a local of `run`, so a substituted body reads the caller's
    // binding — which here IS the right one, because the body lands inside
    // `run`.
    function run(): number {
      const base = 7;
      function add(x: number): number {
        return x + base;
      }
      return add(1) + add(2);
    }
    expect(run()).toBe(17);
  });

  test("a nested helper is not visible after its function returns", () => {
    function makes(): (x: number) => number {
      const inner = (x: number): number => x + 3;
      return inner;
    }
    const held = makes();
    expect(held(1)).toBe(4);
  });

  test("a helper declared inside a function EXPRESSION is substituted", () => {
    // The enclosing function is written in expression position — an IIFE — so
    // the collector had to walk through the expression to find `iifeHelper`.
    const made = (function (): (x: number) => number {
      function iifeHelper(x: number): number {
        return x + 5;
      }
      return (x: number): number => iifeHelper(x) + 1;
    })();
    expect(made(1)).toBe(7);
  });

  test("a body that declares a UNIQUELY named local is substituted", () => {
    function uniqueLocalBody(x: number): number {
      const zqxOnlyHere = x * 3;
      return zqxOnlyHere + 1;
    }
    expect(uniqueLocalBody(2)).toBe(7);
    expect(uniqueLocalBody(0)).toBe(1);
  });

  test("a body local whose name the CALLER also declares does not clobber it", () => {
    // The whole reason the guard exists. `shared` is declared twice in this
    // program, so the count is 2 and the body is refused — and the caller's
    // binding has to survive either way, which is what this asserts.
    function collidingBody(x: number): number {
      const shared = x + 1;
      return shared;
    }
    const shared = 999;
    expect(collidingBody(3)).toBe(4);
    expect(shared).toBe(999);
  });

  test("a body local does not outlive the substitution", () => {
    function declaresOne(x: number): number {
      const zqxScoped = x + 1;
      return zqxScoped;
    }
    expect(declaresOne(1)).toBe(2);
    // Nothing named `zqxScoped` exists out here, and asking is how we find out.
    let seen = "";
    try {
      seen = String(eval("typeof zqxScoped"));
    } catch {
      seen = "undefined";
    }
    expect(seen).toBe("undefined");
  });

  test("two locals in one body, and one of them read by the other", () => {
    function twoLocals(x: number): number {
      const zqxFirst = x + 1;
      const zqxSecond = zqxFirst * 2;
      return zqxSecond;
    }
    expect(twoLocals(3)).toBe(8);
  });

  test("a body that writes a MEMBER writes the caller's object", () => {
    const box: { v: number } = { v: 0 };
    function writesMember(x: number): number {
      box.v = x;
      return x + 1;
    }
    expect(writesMember(7)).toBe(8);
    expect(box.v).toBe(7);
    expect(writesMember(9)).toBe(10);
    expect(box.v).toBe(9);
  });

  test("a body that writes through a PARAMETER writes that object", () => {
    function stamp(target: { v: number }, x: number): number {
      target.v = x;
      return x;
    }
    const first = { v: 0 };
    const second = { v: 0 };
    expect(stamp(first, 1)).toBe(1);
    expect(stamp(second, 2)).toBe(2);
    expect(first.v).toBe(1);
    expect(second.v).toBe(2);
  });

  test("a body that writes an INDEX", () => {
    const slots = [0, 0, 0];
    function putAt(at: number, x: number): number {
      slots[at] = x;
      return x;
    }
    expect(putAt(1, 42)).toBe(42);
    expect(slots.join(",")).toBe("0,42,0");
  });

  test("a body that increments a member", () => {
    const counter: { n: number } = { n: 0 };
    function bumpIt(x: number): number {
      counter.n++;
      return x + 1;
    }
    expect(bumpIt(1)).toBe(2);
    expect(bumpIt(2)).toBe(3);
    expect(counter.n).toBe(2);
  });

  test("a member write happens in the order the call would have", () => {
    const log: string[] = [];
    const sink: { last: number } = { last: 0 };
    function record(x: number): number {
      sink.last = x;
      log.push("w" + x);
      return x;
    }
    log.push("before");
    record(1);
    log.push("between");
    record(2);
    log.push("after");
    expect(log.join(",")).toBe("before,w1,between,w2,after");
    expect(sink.last).toBe(2);
  });
  test("a throw from inside a substituted body propagates", () => {
    function boom(n: number): number {
      const held: any = null;
      return held.x + n;
    }
    let caught = "";
    try {
      boom(1);
    } catch (error) {
      caught = String(error);
    }
    expect(caught.indexOf("TypeError") >= 0).toBe(true);
  });
});

describe("arity and defaults at a substituted call site", () => {
  test("fewer arguments than parameters bind undefined", () => {
    function needsTwo(a: number, b?: number): string {
      return String(a) + "," + String(b) + "," + String(typeof b);
    }
    expect(needsTwo(1)).toBe("1,undefined,undefined");
    expect(needsTwo(1, 2)).toBe("1,2,number");
    function needsThree(a: number, b?: number, c?: number): string {
      return String(a) + String(b) + String(c);
    }
    expect(needsThree(1)).toBe("1undefinedundefined");
    expect(needsThree(1, 2)).toBe("12undefined");
  });

  test("more arguments than parameters are still EVALUATED, in order", () => {
    // A real call evaluates them and drops them, and so must a substituted one:
    // the values have nowhere to go and the side effects still happen.
    const log: string[] = [];
    function mark(tag: string): number {
      log.push(tag);
      return 1;
    }
    function takesOne(a: number): number {
      return a + 1;
    }
    expect((takesOne as any)(mark("a"), mark("b"), mark("c"))).toBe(2);
    expect(log.join(",")).toBe("a,b,c");
  });

  test("a default applies when the argument is absent", () => {
    function withDefault(x: number, y: number = 10): number {
      return x * 100 + y;
    }
    expect(withDefault(1)).toBe(110);
    expect(withDefault(2)).toBe(210);
    // Written, so it takes a real call — and must still be right.
    expect(withDefault(1, 5)).toBe(105);
    expect(withDefault(1, undefined)).toBe(110);
    expect(withDefault(1, 0)).toBe(100);
  });

  test("a default may read a parameter to its LEFT", () => {
    function leaning(a: number, b: number = a + 1, c: number = b * 2): string {
      return String(a) + "," + String(b) + "," + String(c);
    }
    expect(leaning(1)).toBe("1,2,4");
    expect(leaning(5)).toBe("5,6,12");
  });

  test("a default is evaluated ONLY when it is used, and once", () => {
    let made = 0;
    function counting(): number {
      made = made + 1;
      return 7;
    }
    function usesDefault(x: number, y: number = counting()): number {
      return x + y;
    }
    expect(usesDefault(1)).toBe(8);
    expect(made).toBe(1);
    expect(usesDefault(1, 2)).toBe(3);
    expect(made).toBe(1);
    expect(usesDefault(1)).toBe(8);
    expect(made).toBe(2);
  });

  test("a default reading a free name reads the DECLARING scope, not the caller", () => {
    // The hazard the proof closes. A default is emitted at the call site, so a
    // name in it would resolve where the call is written unless the same
    // free-name proof the body gets is asked of the default too.
    let zqxShared = 100;
    function readsShared(x: number, y: number = zqxShared): number {
      return x + y;
    }
    expect(readsShared(1)).toBe(101);
    zqxShared = 200;
    expect(readsShared(1)).toBe(201);
    expect(readsShared(1, 5)).toBe(6);
  });

  test("a default that is an object literal makes a NEW one per call", () => {
    function fresh(x: number, bag: any = { n: 0 }): any {
      bag.n = bag.n + x;
      return bag;
    }
    const first = fresh(1);
    const second = fresh(2);
    expect(first.n).toBe(1);
    expect(second.n).toBe(2);
    expect(first === second).toBe(false);
  });

  test("the values are still right when the callee is not substitutable", () => {
    // Declared twice, so the pass refuses both and these take real calls. The
    // answers must not depend on which path ran.
    function ambiguousArity(a: number, b: number = 3): number {
      return a * 10 + b;
    }
    function elsewhere(): (a: number, b?: number) => number {
      function ambiguousArity(a: number, b: number = 4): number {
        return a * 10 + b;
      }
      return ambiguousArity;
    }
    expect(ambiguousArity(1)).toBe(13);
    expect(elsewhere()(1)).toBe(14);
  });
});

describe("a guard clause in a substituted body", () => {
  test("the guard is taken, and the tail is not", () => {
    function clamped(n: number): number {
      if (n < 0) {
        return 0;
      }
      return n + 1;
    }
    expect(clamped(-5)).toBe(0);
    expect(clamped(0)).toBe(1);
    expect(clamped(5)).toBe(6);
  });

  test("the braceless spelling is the same guard", () => {
    function bare(n: number): number {
      if (n < 0) return 0;
      return n + 1;
    }
    expect(bare(-1)).toBe(0);
    expect(bare(1)).toBe(2);
  });

  test("TWO guards, and the order between them decides", () => {
    function ranged(n: number): string {
      if (n < 0) {
        return "low";
      }
      if (n > 10) {
        return "high";
      }
      return "mid";
    }
    expect(ranged(-1)).toBe("low");
    expect(ranged(5)).toBe("mid");
    expect(ranged(50)).toBe("high");
  });

  test("a guard's answer does NOT see what the statements after it bind", () => {
    // The guard left before them, so the value it produces cannot depend on
    // them — which is why each side starts from the scope the guard started in.
    let seen = "";
    function ordered(n: number): number {
      if (n < 0) {
        seen = seen + "guard";
        return 0;
      }
      const zqxLater = n * 2;
      seen = seen + "tail";
      return zqxLater;
    }
    seen = "";
    expect(ordered(-1)).toBe(0);
    expect(seen).toBe("guard");
    seen = "";
    expect(ordered(3)).toBe(6);
    expect(seen).toBe("tail");
  });

  test("the statements BEFORE a guard run either way", () => {
    const log: string[] = [];
    function staged(n: number): number {
      log.push("before");
      if (n < 0) {
        return 0;
      }
      log.push("after");
      return n;
    }
    log.length = 0;
    expect(staged(-1)).toBe(0);
    expect(log.join(",")).toBe("before");
    log.length = 0;
    expect(staged(1)).toBe(1);
    expect(log.join(",")).toBe("before,after");
  });

  test("a guard reading a body LOCAL declared before it", () => {
    function usesLocal(n: number): number {
      const zqxDoubled = n * 2;
      if (zqxDoubled > 10) {
        return zqxDoubled;
      }
      return zqxDoubled + 1;
    }
    expect(usesLocal(1)).toBe(3);
    expect(usesLocal(9)).toBe(18);
  });

  test("a guard whose condition is not a boolean uses the falsy rule", () => {
    function truthy(v: any): string {
      if (v) {
        return "yes";
      }
      return "no";
    }
    expect(truthy(1)).toBe("yes");
    expect(truthy("a")).toBe("yes");
    expect(truthy([])).toBe("yes");
    expect(truthy(0)).toBe("no");
    expect(truthy("")).toBe("no");
    expect(truthy(null)).toBe("no");
    expect(truthy(undefined)).toBe("no");
    expect(truthy(NaN)).toBe("no");
  });

  test("a guard with an ELSE is not this shape and still answers", () => {
    function withElse(n: number): number {
      if (n < 0) {
        return 0;
      } else {
        return n + 1;
      }
    }
    expect(withElse(-1)).toBe(0);
    expect(withElse(1)).toBe(2);
  });

  test("a guard's condition and answer are evaluated once each, in order", () => {
    const log: string[] = [];
    function mark(tag: string, value: any): any {
      log.push(tag);
      return value;
    }
    function watched(n: number): any {
      if (mark("cond", n < 0)) {
        return mark("answer", 0);
      }
      return mark("tail", n);
    }
    log.length = 0;
    expect(watched(-1)).toBe(0);
    expect(log.join(",")).toBe("cond,answer");
    log.length = 0;
    expect(watched(1)).toBe(1);
    expect(log.join(",")).toBe("cond,tail");
  });

  test("the guard shape composes with a default and with fewer arguments", () => {
    function both(n: number, floorAt: number = 0): number {
      if (n < floorAt) {
        return floorAt;
      }
      return n;
    }
    expect(both(-5)).toBe(0);
    expect(both(5)).toBe(5);
    expect(both(-5, -10)).toBe(-5);
  });
});

describe("substitution must not cycle", () => {
  test("two functions that call each other still compile and answer", () => {
    // The pass refuses a body that mentions its OWN name, which stops `f`
    // calling `f` and says nothing about `f` calling `g` calling `f`. That was
    // hidden for as long as such a body had a `return` in it and `return` was
    // refused; admitting a guard clause removed the hiding place, and the
    // COMPILER overflowed its stack substituting the pair into each other.
    function isEven(n: number): boolean {
      if (n === 0) {
        return true;
      }
      return isOdd(n - 1);
    }
    function isOdd(n: number): boolean {
      if (n === 0) {
        return false;
      }
      return isEven(n - 1);
    }
    expect(isEven(10)).toBe(true);
    expect(isOdd(10)).toBe(false);
    expect(isEven(7)).toBe(false);
    expect(isOdd(7)).toBe(true);
    expect(isEven(0)).toBe(true);
  });

  test("a three-name cycle is refused the same way", () => {
    function first(n: number): number {
      if (n <= 0) {
        return 0;
      }
      return second(n - 1);
    }
    function second(n: number): number {
      if (n <= 0) {
        return 1;
      }
      return third(n - 1);
    }
    function third(n: number): number {
      if (n <= 0) {
        return 2;
      }
      return first(n - 1);
    }
    expect(first(0)).toBe(0);
    expect(first(1)).toBe(1);
    expect(first(2)).toBe(2);
    expect(first(3)).toBe(0);
  });

  test("legitimate nesting is NOT refused, and repeats no name", () => {
    // A stack rather than a depth counter, so a chain that never repeats a name
    // is substituted all the way down.
    function level3(x: number): number {
      return x + 1;
    }
    function level2(x: number): number {
      return level3(x) + 1;
    }
    function level1(x: number): number {
      return level2(x) + 1;
    }
    expect(level1(0)).toBe(3);
    expect(level1(10)).toBe(13);
  });

  test("direct recursion still answers", () => {
    function countDown(n: number): number {
      if (n <= 0) {
        return 0;
      }
      return countDown(n - 1);
    }
    expect(countDown(5)).toBe(0);
    function sum(n: number): number {
      if (n <= 0) {
        return 0;
      }
      return n + sum(n - 1);
    }
    expect(sum(4)).toBe(10);
  });
});

describe("try/catch in a substituted body", () => {
  test("the protected region belongs to the caller and still catches", () => {
    function guarded(x: number): number {
      try {
        const held: any = null;
        return held.v;
      } catch {
        return x + 1;
      }
    }
    expect(guarded(1)).toBe(2);
    expect(guarded(10)).toBe(11);
  });

  test("a catch BINDING does not write the caller's name of that spelling", () => {
    // The catch parameter is a name the body introduces, so it takes the same
    // one-declaration proof the body's locals take. `zqxCaught` exists here and
    // must not be touched.
    function catches(x: number): string {
      try {
        throw new Error("inner");
      } catch (zqxCaught) {
        return String((zqxCaught as Error).message) + String(x);
      }
    }
    expect(catches(1)).toBe("inner1");
  });

  test("a body that THROWS propagates out of the substitution", () => {
    function raises(x: number): number {
      if (x < 0) {
        throw new Error("negative");
      }
      return x + 1;
    }
    expect(raises(1)).toBe(2);
    let caught = "";
    try {
      raises(-1);
    } catch (error) {
      caught = (error as Error).message;
    }
    expect(caught).toBe("negative");
  });

  test("a finally runs on both paths", () => {
    const log: string[] = [];
    function withFinally(x: number): number {
      let seen = 0;
      try {
        seen = x;
        log.push("try");
      } finally {
        log.push("finally");
      }
      return seen + 1;
    }
    log.length = 0;
    expect(withFinally(1)).toBe(2);
    expect(log.join(",")).toBe("try,finally");
  });

  test("try/catch nested inside the caller's own try still separates", () => {
    // The substituted region has to catch its own throw and let the caller's
    // catch see nothing.
    function swallows(x: number): number {
      try {
        throw new Error("mine");
      } catch {
        return x;
      }
    }
    let outer = "none";
    try {
      const answer = swallows(5);
      expect(answer).toBe(5);
    } catch {
      outer = "leaked";
    }
    expect(outer).toBe("none");
  });

  test("a throw the substituted body does NOT catch reaches the caller's catch", () => {
    function raisesOnly(x: number): number {
      try {
        return x + 1;
      } finally {
        // nothing, but the region exists
      }
    }
    function alwaysRaises(x: number): number {
      throw new Error("up:" + String(x));
    }
    expect(raisesOnly(1)).toBe(2);
    let seen = "";
    try {
      alwaysRaises(3);
    } catch (error) {
      seen = (error as Error).message;
    }
    expect(seen).toBe("up:3");
  });
});

describe("a body that mentions a global", () => {
  // A name the whole program declares NOWHERE is resolved through the global
  // object at every site there is, so substituting a body that reads one lands
  // on the same value. `declarations_of` counts zero for it, and zero used to be
  // refused beside two — which meant `Math`, `Error`, `JSON`, `Object` and
  // `console` all refused the body that mentioned them.
  test("Math, and the answer is the global's", () => {
    function usesMath(x: number): number {
      return Math.abs(x) + Math.max(1, 2);
    }
    expect(usesMath(-3)).toBe(5);
    expect(usesMath(4)).toBe(6);
  });

  test("Error, thrown from a guard, keeps its message and type", () => {
    function checked(x: number): number {
      if (x < 0) {
        throw new RangeError("negative: " + String(x));
      }
      return x + 1;
    }
    expect(checked(1)).toBe(2);
    let seen = "";
    let kind = "";
    try {
      checked(-2);
    } catch (error) {
      seen = (error as Error).message;
      kind = error instanceof RangeError ? "RangeError" : "other";
    }
    expect(seen).toBe("negative: -2");
    expect(kind).toBe("RangeError");
  });

  test("JSON and Object, which build values", () => {
    function encoded(x: number): string {
      return JSON.stringify({ v: x });
    }
    expect(encoded(1)).toBe('{"v":1}');
    function keysOf(o: any): number {
      return Object.keys(o).length;
    }
    expect(keysOf({ a: 1, b: 2 })).toBe(2);
  });

  test("a SHADOWED global is not this case and must not be substituted wrongly", () => {
    // Here `Math` is declared — once — so it takes the ordinary one-declaration
    // road, and the body reads the LOCAL one at the site it lands in.
    function outer(): number {
      const Math = { abs: (n: number): number => n * 100 };
      function usesShadowed(x: number): number {
        return Math.abs(x) + 1;
      }
      return usesShadowed(2);
    }
    expect(outer()).toBe(201);
  });

  test("a global that the program ASSIGNS is refused, and still answers", () => {
    // `untouched` is the other half of the proof: a primordial being replaced
    // is the disturbance, so a program that writes one gets no substitution —
    // and the value has to be right either way.
    const before = (globalThis as any).zqxPlanted;
    (globalThis as any).zqxPlanted = 5;
    function readsPlanted(x: number): number {
      return (globalThis as any).zqxPlanted + x;
    }
    expect(readsPlanted(1)).toBe(6);
    (globalThis as any).zqxPlanted = 10;
    expect(readsPlanted(1)).toBe(11);
    (globalThis as any).zqxPlanted = before;
  });

  test("an undeclared name still raises where it would have", () => {
    function readsMissing(x: number): number {
      return (zqxNeverDeclared as any) + x;
    }
    let raised = false;
    try {
      readsMissing(1);
    } catch {
      raised = true;
    }
    expect(raised).toBe(true);
  });
});
declare const zqxNeverDeclared: number;

describe("arguments is the zero-declaration name that is not a global", () => {
  // Every function is given one implicitly, so a body reading `arguments` reads
  // its OWN — and a substituted body would read the CALLER's, which is a
  // different object with different contents. The count cannot see that: it is
  // declared nowhere, so it counts zero exactly as `Math` does.
  //
  // It was safe for as long as zero was refused outright. Admitting zero for
  // globals broke the premise, and `tests/arguments_object.test.ts` said so.
  test("a body reading arguments sees its own", () => {
    function counts(): number {
      return arguments.length;
    }
    expect((counts as any)(1, 2, 3)).toBe(3);
    expect((counts as any)()).toBe(0);
    expect((counts as any)("a")).toBe(1);
  });

  test("and its values, not the caller's", () => {
    function firstOf(): any {
      return arguments[0];
    }
    function wrapper(): any {
      return (firstOf as any)("inner");
    }
    expect((wrapper as any)("outer")).toBe("inner");
  });
});

describe("a guard clause deeper than the top level", () => {
  // `straight_line` admitted a guard at ANY depth while `emit_substituted`
  // intercepted only the top level, so a nested one fell through to
  // `stmt::emit_stmt` and emitted `builder.ret` — a return from the CALLER.
  // `classify(5)` printed nothing and exited zero.
  test("a guard inside an if does not return from the caller", () => {
    function classify(x: number): number {
      if (x > 0) {
        if (x > 10) {
          return 99;
        }
      }
      return x;
    }
    expect(classify(5)).toBe(5);
    expect(classify(50)).toBe(99);
    expect(classify(-1)).toBe(-1);
    // The statement after the call has to run at all, which is what the
    // miscompile took away.
    let reached = false;
    classify(1);
    reached = true;
    expect(reached).toBe(true);
  });

  test("a guard inside a block", () => {
    function blocked(x: number): number {
      {
        if (x > 10) {
          return 99;
        }
      }
      return x;
    }
    expect(blocked(5)).toBe(5);
    expect(blocked(50)).toBe(99);
  });

  test("a guard inside an else", () => {
    function elsed(x: number): number {
      if (x < 0) {
        x = 0;
      } else {
        if (x > 10) {
          return 99;
        }
      }
      return x;
    }
    expect(elsed(-5)).toBe(0);
    expect(elsed(5)).toBe(5);
    expect(elsed(50)).toBe(99);
  });

  test("a guard inside a try is refused along with the rest", () => {
    function tried(x: number): number {
      try {
        if (x > 10) {
          return 99;
        }
      } catch {
        return -1;
      }
      return x;
    }
    expect(tried(5)).toBe(5);
    expect(tried(50)).toBe(99);
  });
});

describe("a void body and a written binding", () => {
  // Two gates that were one refusal each. `body_shape` demanded a trailing
  // `return <expr>`, so a helper that exists for its EFFECTS — 11% of every
  // named function in the corpus — was refused outright; and `closed_over`
  // refused a write to any bound name, on the claim that "a parameter is bound
  // to an SSA value, so a write would have nowhere to land". `Scope::assign`
  // does `entry.1 = Binding::Value(value)` — it rebinds, in the layer the
  // substitution opened.
  test("a void helper runs its effects and answers undefined", () => {
    const log: string[] = [];
    function push(v: string): void {
      log.push(v);
    }
    push("a");
    push("b");
    expect(log.join(",")).toBe("a,b");
    expect(String((push as any)("c"))).toBe("undefined");
    expect(log.join(",")).toBe("a,b,c");
  });

  test("a bare `return;` is the same case written out", () => {
    const log: string[] = [];
    function early(v: number): void {
      log.push("x" + String(v));
      return;
    }
    early(1);
    expect(log.join(",")).toBe("x1");
    expect(String((early as any)(2))).toBe("undefined");
  });

  test("an empty body", () => {
    function nothing(): void {}
    expect(String(nothing())).toBe("undefined");
  });

  test("writing a PARAMETER does not touch the caller's argument", () => {
    function bump(x: number): number {
      x = x + 1;
      return x;
    }
    let held = 5;
    expect(bump(held)).toBe(6);
    expect(held).toBe(5);
  });

  test("an accumulator over a body local", () => {
    function accumulate(n: number): number {
      let total = 0;
      total = total + n;
      total = total + 1;
      return total;
    }
    expect(accumulate(3)).toBe(4);
    expect(accumulate(0)).toBe(1);
  });

  test("a write inside a branch, read after the join", () => {
    function branched(n: number): number {
      let seen = 0;
      if (n > 0) {
        seen = 1;
      }
      return seen + n;
    }
    expect(branched(1)).toBe(2);
    expect(branched(-1)).toBe(-1);
    function both(n: number): number {
      let seen = 0;
      if (n > 0) {
        seen = 1;
      } else {
        seen = 2;
      }
      return seen;
    }
    expect(both(1)).toBe(1);
    expect(both(-1)).toBe(2);
  });

  test("a body local that SHADOWS a caller binding of the same name", () => {
    let zqxShadowed = 500;
    function shadows(n: number): number {
      let zqxShadowed = n * 2;
      zqxShadowed = zqxShadowed + 1;
      return zqxShadowed;
    }
    // Declared twice in the program, so the pass refuses it — and the value has
    // to be right either way, which is what this asserts.
    expect(shadows(3)).toBe(7);
    expect(zqxShadowed).toBe(500);
  });

  test("a void helper whose effect is a write through a free name", () => {
    let zqxCounter = 0;
    function tick(): void {
      zqxCounter = zqxCounter + 1;
    }
    tick();
    tick();
    tick();
    expect(zqxCounter).toBe(3);
  });
});
