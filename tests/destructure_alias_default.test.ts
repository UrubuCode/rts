import { describe, test, expect } from "rts:test";

// (#551) destructuring com alias + default: { key: alias = default }

const { a: aa, b: bb = 88 } = { a: 1 };
const { x: xx = 7, y: yy = 8 } = { y: 20 };
const { p: pp = "fallback" } = {};
const { obj: { z: zz = 5 } = { z: 0 } } = { obj: { z: 99 } };

describe("destructure_alias_default", () => {
  test("alias com default usa value (#551)", () => expect(aa).toBe(1));
  test("alias com default missing key", () => expect(bb).toBe(88));
  test("alias com default ambos missing", () => expect(xx).toBe(7));
  test("alias com default um present", () => expect(yy).toBe(20));
  test("string default", () => expect(pp).toBe("fallback"));
  test("nested alias com default", () => expect(zz).toBe(99));
});
