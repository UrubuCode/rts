import { describe, test, expect } from "rts:test";

// Iterators made while the heap is being collected. Each loop below runs long
// enough that several collections land inside it, so an iterator built at the
// moment one runs is exercised. Three roots were missing (2026-09-27, all
// pre-existing): the list iterator's cell sat in a bare `u32` while its tag was
// interned, `matchAll` held its match arrays in a plain `Vec` while building
// the next, and `iterate` did the same with a Map's entry pairs. The symptoms
// were `undefined is not a function` on a `for-of` row of `bench/analytic.ts`
// that changed from run to run, and `m[1]` reading `undefined` under `matchAll`
// on every run. What this pins is the answers, over enough iterations that the
// old binaries failed every time.

const ROUNDS = 300_000;

describe("iterators survive a collection in flight", () => {
  test("a Set and a Map walked by for-of, values() and entries()", () => {
    const s = new Set<number>();
    const m = new Map<number, string>();
    for (let k = 0; k < 16; k++) { s.add(k); m.set(k, "v" + k); }
    let a = 0, b = 0, c = 0, d = 0;
    for (let i = 0; i < ROUNDS; i++) {
      for (const v of s) a += v;
      for (const v of s.values()) b += v;
      for (const [k, v] of m) c += k + v.length;
      for (const [k] of m.entries()) d += k;
    }
    expect(a).toBe(ROUNDS * 120);
    expect(b).toBe(ROUNDS * 120);
    expect(c).toBe(ROUNDS * (120 + 10 * 2 + 6 * 3));
    expect(d).toBe(ROUNDS * 120);
  });
  test("matchAll over a global pattern, every match with its groups", () => {
    const text = "a1 b2 c3 d4";
    let count = 0, letters = "";
    for (let i = 0; i < 60_000; i++) {
      const r = /([a-z])([0-9])/g;
      for (const found of text.matchAll(r)) {
        count += found[1].length + found[2].length;
        if (i === 59_999) letters += found[1];
      }
    }
    expect(count).toBe(60_000 * 8);
    expect(letters).toBe("abcd");
  });
  test("the manual protocol, a string and a typed array", () => {
    const s = new Set<number>([1, 2, 3, 4]);
    const bytes = new Uint8Array([5, 6, 7, 8]);
    let a = 0, chars = 0, sum = 0;
    for (let i = 0; i < ROUNDS; i++) {
      const it: any = (s as any)[Symbol.iterator]();
      for (;;) { const step = it.next(); if (step.done) break; a += step.value; }
      for (const ch of "héllo") chars += ch.length;
      for (const x of bytes) sum += x;
    }
    expect(a).toBe(ROUNDS * 10);
    expect(chars).toBe(ROUNDS * 5);
    expect(sum).toBe(ROUNDS * 26);
  });
});
