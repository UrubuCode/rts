import { describe, test, expect } from "rts:test";

// toPrecision e toExponential arredondam half-away-from-zero (JS spec),
// nao half-to-even (Rust format!). Inclui cruzamento de ordem de magnitude.
const p25 = (2.5).toPrecision(1);
const p125 = (1.25).toPrecision(2);
const e25 = (2.5).toExponential(0);
const e025 = (0.25).toExponential(0);
const p995 = (9.95).toPrecision(2);   // 9.95 = 9.9499.. em f64 -> "9.9"
const p99_5 = (99.5).toPrecision(2);  // -> "1.0e+2" (exp)
const p9995 = (999.5).toPrecision(3); // -> "1.00e+3"
const p099 = (0.99).toPrecision(1);   // -> "1"
const e999 = (9.99).toExponential(1); // -> "1.0e+1"
// Casos sem cruzamento nao regridem.
const p1234 = (123.456).toPrecision(4);
const e314 = (3.14159).toExponential();
const p100 = (100).toPrecision(3);

describe("toPrecision/toExponential rounding", () => {
  test("2.5.toPrecision(1)", () => expect(p25).toBe("3"));
  test("1.25.toPrecision(2)", () => expect(p125).toBe("1.3"));
  test("2.5.toExponential(0)", () => expect(e25).toBe("3e+0"));
  test("0.25.toExponential(0)", () => expect(e025).toBe("3e-1"));
  test("9.95.toPrecision(2) f64 real", () => expect(p995).toBe("9.9"));
  test("99.5.toPrecision(2) vira exp", () => expect(p99_5).toBe("1.0e+2"));
  test("999.5.toPrecision(3) vira exp", () => expect(p9995).toBe("1.00e+3"));
  test("0.99.toPrecision(1)", () => expect(p099).toBe("1"));
  test("9.99.toExponential(1)", () => expect(e999).toBe("1.0e+1"));
  test("123.456.toPrecision(4) sem cruzar", () => expect(p1234).toBe("123.5"));
  test("3.14159.toExponential()", () => expect(e314).toBe("3.14159e+0"));
  test("100.toPrecision(3)", () => expect(p100).toBe("100"));
});
