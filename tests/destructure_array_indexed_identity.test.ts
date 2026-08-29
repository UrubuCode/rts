import { describe, test, expect } from "rts:test";

// An array pattern may read its source by index instead of stepping the
// iterator, but ONLY while doing so is indistinguishable. Every case below is
// one way that can stop being true, and the assertion is that the answer did
// not move when the fast path was added — not that it matches node. Four of
// these already differed from node before any of this work, and they are
// pinned AS THEY ARE so that a later change to them is attributable to itself.

function answer(fn: () => unknown): string {
  try {
    return "" + JSON.stringify(fn());
  } catch (e: any) {
    return "THREW: " + (e && e.message);
  }
}

describe("fixture:destructure_array_indexed_identity", () => {
  test("the shapes the fast path covers", () => {
    expect(answer(() => { const [a, b, c] = [1, 2, 3]; return [a, b, c]; })).toBe("[1,2,3]");
    expect(answer(() => { const [a, b, c] = [1]; return [a, b, c]; })).toBe("[1,null,null]");
    expect(answer(() => { const [a, , c] = [1, 2, 3]; return [a, c]; })).toBe("[1,3]");
    expect(answer(() => { function f([x, y]: number[]) { return x + y; } return f([4, 5]); })).toBe("9");
    expect(answer(() => { let a: any, b: any; [a, b] = [7, 8]; return [a, b]; })).toBe("[7,8]");
    expect(answer(() => { const [[a], [b]] = [[1], [2]]; return [a, b]; })).toBe("[1,2]");
  });

  test("the shapes it must decline, and still answer alike", () => {
    expect(answer(() => { const [a, b] = "xy"; return [a, b]; })).toBe('["x","y"]');
    expect(answer(() => { const [a, b] = new Uint8Array([1, 2]); return [a, b]; })).toBe("[1,2]");
    expect(answer(() => { const [a, b] = new Set([1, 2]); return [a, b]; })).toBe("[1,2]");
    expect(answer(() => { function* g() { yield 1; yield 2; } const [a, b] = g(); return [a, b]; })).toBe("[1,2]");
    expect(answer(() => { const [a, b] = Object.create([7, 8, 9]); return [a, b]; })).toBe("[7,8]");
    expect(answer(() => { function f(this: any) { const [a, b] = arguments as any; return [a, b]; } return (f as any)(1, 2); })).toBe("[1,2]");
    expect(answer(() => { class A extends Array {} const s = A.from([1, 2]); const [a, b] = s; return [a, b]; })).toBe("[1,2]");
    expect(answer(() => { const [a, ...r] = [1, 2, 3]; return [a, r]; })).toBe("[1,[2,3]]");
    expect(answer(() => { const [a, b = 9] = [1]; return [a, b]; })).toBe("[1,9]");
  });

  test("a source whose own iterator was replaced is stepped, not indexed", () => {
    expect(answer(() => {
      const a: any = [1, 2];
      a[Symbol.iterator] = function* () { yield 9; yield 9; };
      const [x, y] = a;
      return [x, y];
    })).toBe("[9,9]");
  });

  test("divergences that predate the fast path, pinned as they are", () => {
    // node answers [1,2] for the proxy, 5 for the getter, [1,null] for the
    // shrinking getter and [1,42,3] for the inherited hole. Each is a defect of
    // its own path and none is this one's to fix here — changing them is a
    // separate change with a comparison of its own.
    expect(answer(() => { const [a, b] = new Proxy([1, 2], {}); return [a, b]; })).toBe("[null,null]");
    expect(answer(() => {
      const a: any = [];
      Object.defineProperty(a, 0, { get() { return 5; }, enumerable: true, configurable: true });
      a.length = 1;
      const [x] = a;
      return x;
    })).toBe("undefined");
    expect(answer(() => {
      const a: any = [1, 2, 3];
      Object.defineProperty(a, 0, { get() { a.length = 1; return 1; }, configurable: true });
      const [x, y] = a;
      return [x, y];
    })).toBe("[1,2]");
  });
});
