import { describe, test, expect } from "rts:test";

// arr.flat(depth) — recursivo. Infinity = 1024 (clamp).

const deep = [[1, [2, [3, [4]]]]];
const r1 = deep.flat();          // [1, [2, [3, [4]]]]
const r2 = deep.flat(2);         // [1, 2, [3, [4]]]
const r3 = deep.flat(4);
const rInf = deep.flat(Infinity);

const r1_len = r1.length;
const r2_len = r2.length;
const r3_join = r3.join(",");
const rInf_join = rInf.join(",");

describe("array_flat_depth", () => {
  test("flat() len 2", () => expect(r1_len).toBe(2));
  test("flat(2) len 3", () => expect(r2_len).toBe(3));
  test("flat(4) flatten total", () => expect(r3_join).toBe("1,2,3,4"));
  test("flat(Infinity) flatten total", () => expect(rInf_join).toBe("1,2,3,4"));
});
