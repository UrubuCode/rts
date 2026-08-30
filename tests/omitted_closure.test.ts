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

describe("a helper declared inside a block", () => {
  // The declaration is descended into rather than skipped, and the reason it is
  // safe is the reason the whole analysis is: the name is proved dead as a VALUE
  // over the entire function body, not over the block. A binding whose value
  // nothing reads has no observable scope.
  //
  // It matters because the shape is written inside LOOPS, where the cost is paid
  // once per iteration. Measured 2026-08-30, release, min of 9:
  //
  //   for (…) { const q = (x) => x + 1; a = q(a) | 0; }     150.67 -> 8.33
  //   const q = (x) => x + 1; for (…) { a = q(a) | 0; }       8.33  CONTROL
  test("in a loop body, called every iteration", () => {
    function loops(n: number): number {
      let a = 0;
      for (let i = 0; i < n; i++) {
        const qa = (x: number): number => x + 1;
        a = qa(a) | 0;
      }
      return a;
    }
    expect(loops(3)).toBe(3);
    expect(loops(0)).toBe(0);
  });

  test("in an `if` arm", () => {
    function arm(n: number): number {
      let a = 0;
      if (n > 0) {
        const qd = (x: number): number => x * 2;
        a = qd(n);
      }
      return a;
    }
    expect(arm(4)).toBe(8);
    expect(arm(-1)).toBe(0);
  });

  test("in a `try`", () => {
    function guarded(n: number): number {
      let a = 0;
      try {
        const qf = (x: number): number => x + 3;
        a = qf(n);
      } catch {
        a = -1;
      }
      return a;
    }
    expect(guarded(1)).toBe(4);
  });

  test("a loop helper that CAPTURES the loop variable is refused", () => {
    // Each iteration needs its own closure over its own `i`, so the capture
    // clause refuses the name and the closure is built as it always was.
    function captures(n: number): number {
      let a = 0;
      for (let i = 0; i < n; i++) {
        const qb = (x: number): number => x + i;
        a = qb(a) | 0;
      }
      return a;
    }
    expect(captures(3)).toBe(3);
    expect(captures(5)).toBe(10);
  });

  test("one that ESCAPES its block is refused", () => {
    function escapes(): number {
      let held: any;
      {
        const qe = (x: number): number => x + 1;
        held = qe;
      }
      return held(5);
    }
    expect(escapes()).toBe(6);
  });

  test("a nested function's helper belongs to the nested function", () => {
    function outer(n: number): number {
      const inner = function (m: number): number {
        const qh = (x: number): number => x + 1;
        return qh(m);
      };
      return inner(n) + 1;
    }
    expect(outer(1)).toBe(3);
  });
});

describe("a helper that reads a name from around it", () => {
  // The free-name proof used to be a COUNT — every free name declared exactly
  // once in the whole program — and it was taken at collection, so failing it
  // refused the helper outright. The count over-counts on purpose: a parameter,
  // a `catch` binding and a LOOP TARGET all count. So a helper reading its loop
  // variable was refused in every program that has two loops, because both spell
  // it `i`. Measured 2026-08-30, release, min of 9:
  //
  //   for (let i   = …) { const q = (x) => x + i;   … }   233.67 ns
  //   for (let zwq = …) { const q = (x) => x + zwq; … }    46.33 ns
  //
  // The proof is still required. It is asked at the SITE, where `Ctx::omits`
  // offers a stronger one: the helper is declared in this body, is never read as
  // a value, and is not captured — so the caller IS the declarer and a free name
  // resolves to the binding it was written against.
  test("the loop variable, spelled the way every program spells it", () => {
    function reads(n: number): number {
      let a = 0;
      for (let i = 0; i < n; i++) {
        const qi = (x: number): number => x + i;
        a = qi(a) | 0;
      }
      return a;
    }
    // A second loop over `i`, so the count says two and the old gate refused.
    function elsewhere(n: number): number {
      let s = 0;
      for (let i = 0; i < n; i++) s = (s + 1) | 0;
      return s;
    }
    expect(reads(4)).toBe(6);
    expect(elsewhere(4)).toBe(4);
  });

  test("it reads the value the iteration has, not the last one", () => {
    // The whole hazard of moving a closure body: if the substituted read
    // resolved to something other than this iteration's binding, this answers 9.
    function each(): string {
      const seen: number[] = [];
      for (let i = 0; i < 3; i++) {
        const qj = (): number => i;
        seen.push(qj());
      }
      return seen.join(",");
    }
    expect(each()).toBe("0,1,2");
  });

  test("an outer binding written between two calls", () => {
    function changes(): number {
      let held = 1;
      const qk = (x: number): number => x + held;
      const first = qk(0);
      held = 10;
      const second = qk(0);
      return first * 100 + second;
    }
    expect(changes()).toBe(110);
  });

  test("a parameter of the enclosing function", () => {
    function outer(n: number): number {
      const ql = (x: number): number => x + n;
      return ql(1);
    }
    expect(outer(5)).toBe(6);
  });

  test("a name the helper both reads and WRITES", () => {
    function accumulates(n: number): number {
      let total = 0;
      for (let i = 0; i < n; i++) {
        const qm = (x: number): void => {
          total = total + x;
        };
        qm(i);
      }
      return total;
    }
    expect(accumulates(4)).toBe(6);
  });

  test("a shadowing binding is still the inner one", () => {
    function shadows(): number {
      const held = 1;
      function inner(): number {
        const held2 = 100;
        const qn = (x: number): number => x + held2;
        return qn(0);
      }
      return inner() + held;
    }
    expect(shadows()).toBe(101);
  });

  test("a helper reading a global still answers through it", () => {
    function usesGlobal(n: number): number {
      const qo = (x: number): number => Math.abs(x) + n;
      return qo(-5);
    }
    expect(usesGlobal(1)).toBe(6);
  });
});
