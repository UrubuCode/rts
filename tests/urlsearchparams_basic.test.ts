import { describe, test, expect } from "rts:test";

// `new URLSearchParams("a=1&b=2")` (Registry-class) + get/has/set/delete.
// Resultados devem casar com bun/node.

const p = new URLSearchParams("a=1&b=2");
const a = p.get("a");
const b = p.get("b");
const hasA = p.has("a");
const hasZ = p.has("z");
p.set("c", "3");
const c = p.get("c");

describe("fixture:urlsearchparams_basic", () => {
  test("get parsed value", () => {
    expect(a).toBe("1");
  });
  test("get second value", () => {
    expect(b).toBe("2");
  });
  test("has present key", () => {
    expect(hasA).toBe(true);
  });
  test("has absent key", () => {
    expect(hasZ).toBe(false);
  });
  test("set then get", () => {
    expect(c).toBe("3");
  });
});
