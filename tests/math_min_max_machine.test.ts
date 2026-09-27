import { describe, test, expect } from "rts:test";

// `Math.min(a, b)` and `Math.max(a, b)` over two numbers are one machine
// instruction. What that instruction must still get right is the two cases a
// plain comparison gets wrong, because IEEE says `-0 < +0` is false and any
// comparison with NaN is false:
//
//   Math.min(0, -0)  is -0   (from either side)
//   Math.max(-0, 0)  is +0   (from either side)
//   Math.min(NaN, 1) is NaN  (from either side)
//
// The other arities are a different path — a call — and are pinned here so a
// two-argument instruction cannot quietly become the only form that works.

function neg0(): number { return -0; }
function pos0(): number { return 0; }
function nan(): number { return NaN; }

// A value nothing proves a number, so the instruction is refused and the call
// runs instead — and `f` must run ONCE. On the tree before this fixture the
// refused argument was emitted twice: `calls` read 2.
let calls = 0;
function loud(): any { calls++; return calls > 100 ? "x" : 2.5; }

describe("Math.min / Math.max as the machine's instruction", () => {
  test("min keeps -0 from either side", () => {
    expect(Object.is(Math.min(pos0(), neg0()), -0)).toBe(true);
    expect(Object.is(Math.min(neg0(), pos0()), -0)).toBe(true);
  });
  test("max keeps +0 from either side", () => {
    expect(Object.is(Math.max(neg0(), pos0()), 0)).toBe(true);
    expect(Object.is(Math.max(pos0(), neg0()), 0)).toBe(true);
  });
  test("NaN propagates from either side", () => {
    expect(Number.isNaN(Math.min(nan(), 1))).toBe(true);
    expect(Number.isNaN(Math.min(1, nan()))).toBe(true);
    expect(Number.isNaN(Math.max(nan(), 1))).toBe(true);
    expect(Number.isNaN(Math.max(1, nan()))).toBe(true);
  });
  test("ordinary operands", () => {
    let acc = 0;
    for (let i = 0; i < 10; i++) acc += Math.min(i, 7) + Math.max(i & 3, 2);
    expect(acc).toBe(0 + 1 + 2 + 3 + 4 + 5 + 6 + 7 + 7 + 7 + (2 + 2 + 2 + 3) * 2 + 2 + 2);
    expect(Math.min(2.5, 1.5)).toBe(1.5);
    expect(Math.max(-1.5, -2.5)).toBe(-1.5);
    expect(Math.min(Infinity, 3)).toBe(3);
    expect(Math.max(-Infinity, 3)).toBe(3);
  });
  test("other arities are still the fold", () => {
    expect(Math.min()).toBe(Infinity);
    expect(Math.max()).toBe(-Infinity);
    expect(Math.min(4)).toBe(4);
    expect(Math.max(1, 9, 3)).toBe(9);
    expect(Math.min(1, 9, -3, 4)).toBe(-3);
  });
  test("a refused argument is evaluated once", () => {
    const got = Math.min(loud(), 7);
    expect(got).toBe(2.5);
    expect(calls).toBe(1);
    const floored = Math.floor(loud());
    expect(floored).toBe(2);
    expect(calls).toBe(2);
    expect(Math.max("3" as any, 2)).toBe(3);
  });
});
