import { describe, test, expect } from "rts:test";

// new Set([elements]) e new Map([[k,v],...]) com initial.

const s = new Set([1, 2, 2, 3, 4]);
const ssize = s.size;
const sh2 = s.has(2);
const sh5 = s.has(5);

const m = new Map([["a", 1], ["b", 2]]);
const msize = m.size;
const ma = m.get("a");
const mb = m.get("b");

describe("set_map_init_array", () => {
  test("Set dedupes init", () => expect(ssize).toBe(4));
  test("Set has init element", () => expect(sh2).toBe(true));
  test("Set has missing", () => expect(sh5).toBe(false));
  test("Map size from init", () => expect(msize).toBe(2));
  test("Map.get(a) = 1", () => expect(ma).toBe(1));
  test("Map.get(b) = 2", () => expect(mb).toBe(2));
});
