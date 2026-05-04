import { describe, test, expect } from "rts:test";

// Math.max() / Math.min() sem args sao -Infinity / +Infinity (JS spec).

const a = Math.max();
const b = Math.min();
const a_inf = a === -Infinity;
const b_inf = b === Infinity;

describe("math_max_min_empty", () => {
  test("max() = -Infinity", () => expect(a_inf).toBe(true));
  test("min() = Infinity", () => expect(b_inf).toBe(true));
});
