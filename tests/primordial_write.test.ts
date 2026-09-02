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
