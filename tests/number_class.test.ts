import { describe, test, expect } from "rts:test";

// ── Static methods ────────────────────────────────────────────────────────────
describe("number_class/static", () => {
  test("isNaN", () => {
    expect(Number.isNaN(NaN)).toBe(true);
    expect(Number.isNaN(42)).toBe(false);
    expect(Number.isNaN(Number.NaN)).toBe(true);
  });

  test("isFinite", () => {
    expect(Number.isFinite(42.0)).toBe(true);
    expect(Number.isFinite(Infinity)).toBe(false);
    expect(Number.isFinite(Number.POSITIVE_INFINITY)).toBe(false);
    expect(Number.isFinite(Number.NEGATIVE_INFINITY)).toBe(false);
  });

  test("isInteger", () => {
    expect(Number.isInteger(3.0)).toBe(true);
    expect(Number.isInteger(3.5)).toBe(false);
  });

  test("isSafeInteger", () => {
    expect(Number.isSafeInteger(Number.MAX_SAFE_INTEGER)).toBe(true);
    expect(Number.isSafeInteger(Number.MAX_SAFE_INTEGER + 1)).toBe(false);
  });
});

// ── Constants ─────────────────────────────────────────────────────────────────
describe("number_class/constants", () => {
  test("MAX_SAFE_INTEGER", () =>
    expect(Number.MAX_SAFE_INTEGER === 9007199254740991).toBe(true));

  test("MIN_SAFE_INTEGER", () =>
    expect(Number.MIN_SAFE_INTEGER === -9007199254740991).toBe(true));
});

// ── Number() coercion ─────────────────────────────────────────────────────────
describe("number_class/coercion", () => {
  test("Number(string) — numeros validos", () => {
    expect(Number("42") === 42).toBe(true);
    expect(Number("3.14") === 3.14).toBe(true);
  });

  test("Number('') === 0 (JS spec)", () =>
    expect(Number("") === 0).toBe(true));

  test("Number('abc') === NaN", () =>
    expect(isNaN(Number("abc"))).toBe(true));

  test("Number() === 0", () =>
    expect(Number() === 0).toBe(true));
});

// ── Instance methods ──────────────────────────────────────────────────────────
describe("number_class/instance", () => {
  test("toFixed", () =>
    expect((3.14159).toFixed(2)).toBe("3.14"));

  test("toString() — base 10", () =>
    expect((1000).toString()).toBe("1000"));

  test("toString(16) — hex", () =>
    expect((255).toString(16)).toBe("ff"));

  test("toString(2) — binario", () =>
    expect((8).toString(2)).toBe("1000"));

  test("toExponential", () =>
    expect((1.5e10).toExponential(2)).toBe("1.50e+10"));

  test("toPrecision", () =>
    expect((123.456).toPrecision(5)).toBe("123.46"));

  test("valueOf", () =>
    expect((42.0).valueOf() === 42).toBe(true));
});

// ── Global isNaN / isFinite ───────────────────────────────────────────────────
describe("number_class/globals", () => {
  test("global isNaN", () => {
    expect(isNaN(NaN)).toBe(true);
    expect(isNaN(0)).toBe(false);
  });

  test("global isFinite", () => {
    expect(isFinite(1.0)).toBe(true);
    expect(isFinite(Infinity)).toBe(false);
  });
});
