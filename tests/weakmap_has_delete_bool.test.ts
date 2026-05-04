import { describe, test, expect } from "rts:test";

// (#554) WeakMap/WeakSet has/delete devem retornar Bool, nao i64 cru.

const wm = new WeakMap();
const k = { id: 1 };
wm.set(k, 99);
const h1 = wm.has(k);          // true
const d1 = wm.delete(k);       // true
const h2 = wm.has(k);          // false

const ws = new WeakSet();
const k2 = { id: 2 };
ws.add(k2);
const h3 = ws.has(k2);         // true
const d2 = ws.delete(k2);      // true
const h4 = ws.has(k2);         // false

describe("weakmap_has_delete_bool", () => {
  test("WeakMap.has true (#554)", () => expect(h1).toBe(true));
  test("WeakMap.delete true (#554)", () => expect(d1).toBe(true));
  test("WeakMap.has after delete false", () => expect(h2).toBe(false));
  test("WeakSet.has true", () => expect(h3).toBe(true));
  test("WeakSet.delete true", () => expect(d2).toBe(true));
  test("WeakSet.has after delete false", () => expect(h4).toBe(false));
});
