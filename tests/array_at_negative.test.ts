import { describe, test, expect } from "rts:test";

// arr.at(idx) com idx negativo: idx + len.

const a = [10, 20, 30, 40, 50];
const r0 = a.at(0);
const r2 = a.at(2);
const rn1 = a.at(-1);
const rn2 = a.at(-2);
const rn5 = a.at(-5);

describe("array_at_negative", () => {
  test("at(0)", () => expect(r0).toBe(10));
  test("at(2)", () => expect(r2).toBe(30));
  test("at(-1)", () => expect(rn1).toBe(50));
  test("at(-2)", () => expect(rn2).toBe(40));
  test("at(-5)", () => expect(rn5).toBe(10));
});
