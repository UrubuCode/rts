// A default parameter is CODE, and the names it reads have to be captured.
//
// `capture::names_in_function` walked each parameter's TARGET, which answers
// what the pattern binds, and never its DEFAULT, which is the other question.
// So an enclosing function never put the name in its environment and the
// default's read found nothing:
//
//     function outer() {
//       let shared = 100;
//       const g = (x, y = shared) => x + y;
//       g(1);        // ReferenceError: shared is not defined
//     }
//
// Only when the default was actually EVALUATED — `g(1, 5)` worked and `g(1)`
// did not — which is why a corpus of programs that pass their arguments never
// caught it.
import { describe, test, expect } from "rts:test";

describe("a default parameter reading an enclosing binding", () => {
  test("a function declaration", () => {
    function outer(): number[] {
      let shared = 100;
      function inner(x: number, y: number = shared): number {
        return x + y;
      }
      const first = inner(1);
      shared = 200;
      return [first, inner(1), inner(1, 5)];
    }
    expect(outer().join(",")).toBe("101,201,6");
  });

  test("an arrow, which no substitution can reach", () => {
    function outer(): number {
      const shared = 7;
      const inner = (x: number, y: number = shared): number => x + y;
      return inner(1);
    }
    expect(outer()).toBe(8);
  });

  test("a function that ESCAPES as a value, so nothing is substituted", () => {
    function outer(): (x: number) => number {
      const held = 4;
      function inner(x: number, y: number = held): number {
        return x + y;
      }
      return inner;
    }
    expect(outer()(1)).toBe(5);
  });

  test("the FIRST parameter having a default", () => {
    function outer(): number {
      const base = 3;
      function inner(x: number = base * 2): number {
        return x;
      }
      return inner();
    }
    expect(outer()).toBe(6);
  });

  test("a default that CALLS an enclosing function", () => {
    function outer(): number {
      function makes(): number {
        return 9;
      }
      function inner(x: number, y: number = makes()): number {
        return x + y;
      }
      return inner(1);
    }
    expect(outer()).toBe(10);
  });

  test("a default inside a destructuring pattern", () => {
    // `{ a = seen }` reads `seen` with no `parameter.default` in sight — the
    // default lives inside the pattern, and only `walk_pattern_exprs` knows
    // where those are.
    function outer(): number {
      const seen = 5;
      function inner({ a = seen }: { a?: number } = {}): number {
        return a;
      }
      return inner() + inner({ a: 1 });
    }
    expect(outer()).toBe(6);
  });

  test("a default reading the enclosing binding through two levels", () => {
    function outermost(): number {
      const deep = 11;
      function middle(): number {
        function inner(x: number, y: number = deep): number {
          return x + y;
        }
        return inner(1);
      }
      return middle();
    }
    expect(outermost()).toBe(12);
  });

  test("the enclosing binding is read at CALL time, not at definition time", () => {
    function outer(): string {
      let counter = 0;
      function inner(x: number, y: number = counter): number {
        return x + y;
      }
      const before = inner(1);
      counter = 10;
      const after = inner(1);
      return String(before) + "," + String(after);
    }
    expect(outer()).toBe("1,11");
  });

  test("a method's default, and a class field's", () => {
    function outer(): number {
      const gift = 6;
      const holder = {
        take(x: number, y: number = gift): number {
          return x + y;
        },
      };
      class Box {
        take(x: number, y: number = gift): number {
          return x + y;
        }
      }
      return holder.take(1) + new Box().take(1);
    }
    expect(outer()).toBe(14);
  });
});
