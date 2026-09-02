// `primordial::untouched` decides whether `Math.sqrt(x)` may become a machine
// instruction. It answers by walking for a write to `Math` or to a member of it
// — and a TypeScript `as` cast parses to `ExprKind::Asserted`, so
// `(Math as any).sqrt = f` is a member expression whose OBJECT is an assertion
// rather than the identifier. Comparing that object against `ExprKind::Ident`
// answered no, and the write was invisible:
//
//   Math.sqrt = () => 42;           Math.sqrt(16)   ->  42, correct
//   (Math as any).sqrt = () => 42;  Math.sqrt(16)   ->   4, WRONG
//
// Both are the same program, and the second is how the write is spelled in the
// language this engine compiles. A claim carries no value and evaluates nothing,
// so stepping through it can never skip an effect.
import { describe, test, expect } from "rts:test";

describe("a write the emitter must see through a cast", () => {
  test("a plain write is seen", () => {
    const held = Math.sqrt;
    try {
      Math.sqrt = () => 42;
      expect(Math.sqrt(16)).toBe(42);
    } finally {
      Math.sqrt = held;
    }
  });

  test("a write through `as any` is seen too", () => {
    const held = Math.sqrt;
    try {
      (Math as any).sqrt = () => 42;
      expect(Math.sqrt(16)).toBe(42);
    } finally {
      Math.sqrt = held;
    }
  });

  test("a doubled cast is seen", () => {
    const held = Math.floor;
    try {
      ((Math as unknown) as any).floor = () => 99;
      expect(Math.floor(3.7)).toBe(99);
    } finally {
      Math.floor = held;
    }
  });

  test("a non-null assertion is seen", () => {
    const held = Math.abs;
    try {
      (Math! as any).abs = () => 7;
      expect(Math.abs(-3)).toBe(7);
    } finally {
      Math.abs = held;
    }
  });

  test("and an undisturbed `Math` still answers arithmetic", () => {
    // The falsifier for the fix rather than the bug: if the walk were made too
    // eager, every program would lose the instruction lowering.
    let v = 16.0;
    expect(Math.sqrt(v)).toBe(4);
  });
});

describe("the write forms a place expression is not", () => {
  // `Disturbance::expression` matched only `AssignTarget::Place`, so a
  // DESTRUCTURING assignment reached `names_it` through none of its leaves; and
  // `walk_stmt`'s own documentation says it is SILENT about a for-each target,
  // so `for (x of xs)` was invisible to every walk built on it.
  //
  // It was a silent wrong answer in ONE file, through `inline::candidates`
  // asking the same question about an ordinary function:
  //
  //   function zf(x) { return x + 1; }
  //   [zf] = [(x) => x + 100];
  //   zf(1)                       // 2 here, 101 in node, exit 0
  //
  // The original body was spliced at the call because nothing had seen the
  // rewrite. The same two forms defeated the `Math` fold.
  test("a destructuring assignment to a function", () => {
    function zf(x: number): number {
      return x + 1;
    }
    [zf] = [(x: number) => x + 100];
    expect(zf(1)).toBe(101);
  });

  test("a `for`-`of` target that assigns rather than declares", () => {
    function zg(x: number): number {
      return x + 1;
    }
    for (zg of [(x: number) => x + 100]) {
      // the loop exists for its target, not its body
    }
    expect(zg(1)).toBe(101);
  });

  test("a destructuring write to a primordial's member", () => {
    const held = Math.sqrt;
    try {
      [Math.sqrt] = [() => 42];
      let v = 16.0;
      expect(Math.sqrt(v)).toBe(42);
    } finally {
      Math.sqrt = held;
    }
  });

  test("an object pattern reaches its leaves too", () => {
    function zh(x: number): number {
      return x + 1;
    }
    ({ zh } = { zh: (x: number) => x + 100 });
    expect(zh(1)).toBe(101);
  });

  test("a nested pattern, and a rest element", () => {
    function zi(x: number): number {
      return x + 1;
    }
    function zj(x: number): number {
      return x + 1;
    }
    [[zi], [...[zj]]] = [[(x: number) => x + 100], [(x: number) => x + 200]];
    expect(zi(1)).toBe(101);
    expect(zj(1)).toBe(201);
  });

  test("and a `for`-`of` that DECLARES still writes nothing", () => {
    // The falsifier for the fix: `for (const x of xs)` introduces a binding and
    // touches nothing outside, so the fold must survive it.
    let seen = 0;
    for (const step of [1, 2, 3]) seen += step;
    expect(seen).toBe(6);
    let v = 16.0;
    expect(Math.sqrt(v)).toBe(4);
  });
});
