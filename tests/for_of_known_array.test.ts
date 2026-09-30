import { describe, test, expect } from "rts:test";

// A `for-of` over an array the program shows to be one walks the array itself,
// without asking per pass what its iterator is. What this pins is that nothing
// a program can see changed: order, `break`, holes, a source that is not an
// array, and a walk from a nested function over the same array.

const xs = [1, 2, 3];
const holes = [1, , 3] as number[];

describe("for-of over an array the program knows", () => {
  test("visits every element in order, and stops at break", () => {
    let sum = 0;
    for (let i = 0; i < 1000; i++) for (const v of xs) sum += v;
    expect(sum).toBe(6000);
    let seen = 0;
    for (const v of xs) { seen++; if (v === 2) break; }
    expect(seen).toBe(2);
    let joined = "";
    for (const v of holes) joined += String(v) + ",";
    expect(joined).toBe("1,undefined,3,");
    const local = [4, 5];
    let acc = 0;
    for (const v of local) acc = acc * 10 + v;
    expect(acc).toBe(45);
    const read = () => { let t = 0; for (const v of xs) t += v; return t; };
    expect(read()).toBe(6);
  });
  test("a source that is not an array is stepped", () => {
    const set = new Set([7, 8]);
    let s = 0;
    for (const v of set) s += v;
    expect(s).toBe(15);
    let text = "";
    for (const c of "ab") text += c;
    expect(text).toBe("ab");
    class It { *[Symbol.iterator]() { yield 9; yield 10; } }
    let it = 0;
    for (const v of new It()) it += v;
    expect(it).toBe(19);
  });
  test("the length is read on every step: a push is visited, a truncation ends it", () => {
    // The language's array iterator reads `length` per step. The running
    // emitter hoisted it once and walked three of five (found 2026-09-30).
    const xs = [1, 2, 3];
    const seen: number[] = [];
    for (const x of xs) { seen.push(x); if (x < 3) xs.push(x + 10); }
    expect(seen.join()).toBe("1,2,3,11,12");
    const ys = [1, 2, 3, 4];
    const popped: number[] = [];
    for (const y of ys) { popped.push(y); if (y === 1) ys.pop(); }
    expect(popped.join()).toBe("1,2,3");
    const zs = [1, 2, 3];
    const cut: number[] = [];
    for (const z of zs) { cut.push(z); if (z === 1) zs.length = 0; }
    expect(cut.join()).toBe("1");
  });
});
