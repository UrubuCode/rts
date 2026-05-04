import { describe, test, expect } from "rts:test";

// (#550) Boolean("") deve retornar false (string vazia e' falsy).

const a = Boolean("");
const b = Boolean("x");
const c = Boolean(NaN);
const d = Boolean(0);
const e = Boolean(42);

describe("boolean_coerce_string", () => {
  test("empty string -> false (#550)", () => expect(a).toBe(false));
  test("non-empty string -> true", () => expect(b).toBe(true));
  test("NaN -> false", () => expect(c).toBe(false));
  test("0 -> false", () => expect(d).toBe(false));
  test("42 -> true", () => expect(e).toBe(true));
});
