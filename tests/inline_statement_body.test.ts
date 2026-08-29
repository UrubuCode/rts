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
