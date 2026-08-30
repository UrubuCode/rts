// A helper whose closure nothing reads is not built at all.
//
// `omit::omittable` proves, once per body and before any of it is emitted, that
// every call to a `const f = <function>` is substituted — so the object would be
// allocated, rooted, and never looked at. Measured 2026-08-30: a nested arrow
// called once per call of its enclosing function cost 180.67 ns against 8.33 for
// the same helper written outside it.
//
// It OMITS rather than DEFERS. Deferring would move WHEN an allocation happens,
// and this engine finds GC roots by a conservative scan whose result depends on
// allocation order — see `docs/engine/lost-roots.md`. Omitting is decided
// entirely at compile time.
//
// The proof has to be COMPLETE: nothing is bound, so a call that fell back to a
// real one would read a name that does not exist. Every test below that expects
// an ordinary answer is therefore a test that the refusal fired.
import { describe, test, expect } from "rts:test";

describe("a helper whose closure nobody reads", () => {
  test("the shape it exists for", () => {
    function usesNested(x: number): number {
      const step = (y: number): number => y + 1;
      return step(x);
    }
    expect(usesNested(1)).toBe(2);
    expect(usesNested(10)).toBe(11);
  });

  test("called more than once, and nested in itself", () => {
    function twice(x: number): number {
      const add = (y: number): number => y + 2;
      return add(add(x));
    }
    expect(twice(1)).toBe(5);
  });

  test("several helpers in one body", () => {
    function many(x: number): number {
      const a = (y: number): number => y + 1;
      const b = (y: number): number => y * 2;
      return b(a(x));
    }
    expect(many(3)).toBe(8);
  });
});

describe("what the omission must refuse", () => {
  // Each of these reads the VALUE, so the closure has to exist. An engine that
  // omitted it here would fail at compile time with an unbound name — loud
  // rather than wrong — but the point is to answer correctly.
  test("typeof it, and its own properties", () => {
    function reads(x: number): string {
      const f = (y: number): number => y + 1;
      return String(f(x)) + typeof f + String(f.name) + String(f.length);
    }
    expect(reads(1)).toBe("2functionf1");
  });

  test("passed as an argument", () => {
    function passes(x: number): number {
      const g = (y: number): number => y + 1;
      const apply = (h: any, v: number): number => h(v);
      return apply(g, x);
    }
    expect(passes(1)).toBe(2);
  });

  test("assigned to another binding", () => {
    function aliased(x: number): number {
      const f = (y: number): number => y + 1;
      const held = f;
      return held(x) + f(x);
    }
    expect(aliased(1)).toBe(4);
  });

  test("returned, so it outlives the call", () => {
    function makes(): (y: number) => number {
      const f = (y: number): number => y + 1;
      return f;
    }
    expect(makes()(1)).toBe(2);
  });

  test("put in an object or an array", () => {
    function collects(x: number): number {
      const f = (y: number): number => y + 1;
      const bag = { f };
      const list = [f];
      return bag.f(x) + list[0](x);
    }
    expect(collects(1)).toBe(4);
  });

  test("called optionally, which tests the value first", () => {
    function optional(x: number): number {
      const f = (y: number): number => y + 1;
      return f?.(x) ?? 0;
    }
    expect(optional(1)).toBe(2);
  });

  test("read by a nested function, which captures it", () => {
    function captured(x: number): number {
      const f = (y: number): number => y + 1;
      const outer = (): number => f(x);
      return outer();
    }
    expect(captured(1)).toBe(2);
  });

  test("compared, which is a read", () => {
    function compares(): boolean {
      const f = (y: number): number => y + 1;
      return f === f;
    }
    expect(compares()).toBe(true);
  });

  test("a helper that CALLS another one", () => {
    // Refused because a call inside the body can hit the cycle check and fall
    // back — and a fallback needs the binding.
    function chained(x: number): number {
      const inner = (y: number): number => y + 1;
      const outer = (y: number): number => inner(y) + 1;
      return outer(x);
    }
    expect(chained(1)).toBe(3);
  });

  test("a defaulted parameter, and a rest parameter", () => {
    function defaulted(x: number): number {
      const f = (y: number, z: number = 10): number => y + z;
      return f(x) + f(x, 1);
    }
    expect(defaulted(1)).toBe(13);
    function rested(x: number): number {
      const f = (...ys: number[]): number => ys.length;
      return f(x, x, x);
    }
    expect(rested(1)).toBe(3);
  });

  test("called with a spread", () => {
    function spread(x: number): number {
      const f = (y: number): number => y + 1;
      const args: number[] = [x];
      return f(...args);
    }
    expect(spread(1)).toBe(2);
  });

  test("a helper declared inside a BLOCK, not at the top level", () => {
    function blocked(x: number): number {
      let out = 0;
      {
        const f = (y: number): number => y + 1;
        out = f(x);
      }
      return out;
    }
    expect(blocked(1)).toBe(2);
  });

  test("a `var` helper, which hoisting already bound", () => {
    function varied(x: number): number {
      var f = function (y: number): number {
        return y + 1;
      };
      return f(x);
    }
    expect(varied(1)).toBe(2);
  });

  test("shadowed by another declaration of the same name elsewhere", () => {
    function first(x: number): number {
      const zqxSame = (y: number): number => y + 1;
      return zqxSame(x);
    }
    function second(x: number): number {
      const zqxSame = (y: number): number => y + 2;
      return zqxSame(x);
    }
    expect(first(1)).toBe(2);
    expect(second(1)).toBe(3);
  });
});
