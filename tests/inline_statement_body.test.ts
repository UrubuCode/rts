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
