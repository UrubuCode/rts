// `capture::names_in_function` inserted a nested function's OWN NAME into the
// set of names its body mentions. That set is intersected with what the
// ENCLOSING function declares, so a nested `function unused() {}` put `unused`
// in the enclosing function's environment — an object allocated and filled on
// every call, for a binding nothing reads.
//
// A nested function that recurses writes its own name IN ITS BODY, so the walk
// finds it either way; a sibling that calls it writes the name in hers. The
// insert only added the case where nothing mentions it at all.
//
// This is the highest-risk analysis in the emitter — the module header records a
// wrong answer it once produced — so these cases are about VALUES, and every one
// was checked against node first.
import { describe, test, expect } from "rts:test";

describe("what a nested function makes its enclosing function capture", () => {
  test("a nested function nothing references", () => {
    function outer(): number {
      function unused(y: number): number {
        return y;
      }
      return 1;
    }
    expect(outer()).toBe(1);
  });

  test("one that recurses by its own name", () => {
    function outer(): number {
      function down(n: number): number {
        return n <= 0 ? 0 : down(n - 1);
      }
      return down(3);
    }
    expect(outer()).toBe(0);
  });

  test("a sibling that calls it", () => {
    function outer(): number {
      function one(): number {
        return 1;
      }
      function two(): number {
        return one() + 1;
      }
      return two();
    }
    expect(outer()).toBe(2);
  });

  test("a nested function that READS an enclosing binding after it changes", () => {
    function outer(): number {
      let held = 5;
      function reads(): number {
        return held;
      }
      held = 7;
      return reads();
    }
    expect(outer()).toBe(7);
  });

  test("a nested function that WRITES an enclosing binding", () => {
    function outer(): number {
      let count = 0;
      function bump(): void {
        count = count + 1;
      }
      bump();
      bump();
      return count;
    }
    expect(outer()).toBe(2);
  });

  test("a named function EXPRESSION recursing by its inner name", () => {
    function outer(): number {
      const held = function fact(n: number): number {
        return n <= 1 ? 1 : n * fact(n - 1);
      };
      return held(4);
    }
    expect(outer()).toBe(24);
  });

  test("a nested parameter that SHADOWS one of the enclosing function's", () => {
    function outer(x: number): number {
      function inner(x: number): number {
        return x * 10;
      }
      return inner(2) + x;
    }
    expect(outer(3)).toBe(23);
  });

  test("the block-scoped shadowing the module header records", () => {
    // `run` answered `undefined` where node answers 1, because a name declared
    // inside a nested BLOCK was over-included and then shadowed the outer
    // binding for the whole function. It must keep answering 1.
    function run(): number {
      let v = 1;
      {
        let v = 10;
        function inner(): number {
          return v;
        }
        expect(inner()).toBe(10);
      }
      return v;
    }
    expect(run()).toBe(1);
  });

  test("two closures over one variable still share it", () => {
    function outer(): string {
      let shared = 0;
      function up(): void {
        shared = shared + 1;
      }
      function read(): number {
        return shared;
      }
      up();
      up();
      return String(read());
    }
    expect(outer()).toBe("2");
  });

  test("a nested function returned as a value keeps its capture", () => {
    function makes(): () => number {
      let held = 41;
      function reads(): number {
        return held + 1;
      }
      held = 41;
      return reads;
    }
    expect(makes()()).toBe(42);
  });
});
