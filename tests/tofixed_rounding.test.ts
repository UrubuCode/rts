import { describe, test, expect } from "rts:test";

// toFixed arredonda half-away-from-zero sobre o valor decimal real do f64
// (JS spec), nao half-to-even (Rust format!). E preserva o sinal ao zerar.
const r05 = (0.5).toFixed(0);
const r15 = (1.5).toFixed(0);
const r25 = (2.5).toFixed(0);
const rneg25 = (-2.5).toFixed(0);
const r0125 = (0.125).toFixed(2);
// 2.55 e 1.45 sao 2.5499.. / 1.4499.. em f64 -> arredondam pra baixo.
const r255 = (2.55).toFixed(1);
const r145 = (1.45).toFixed(1);
const negZero = (-0.001).toFixed(2);
const r1005 = (1.005).toFixed(2);
const big = (1234.5678).toFixed(2);
const padZeros = (100).toFixed(2);

describe("toFixed rounding (JS half-away-from-zero)", () => {
  test("0.5 -> 1", () => expect(r05).toBe("1"));
  test("1.5 -> 2", () => expect(r15).toBe("2"));
  test("2.5 -> 3", () => expect(r25).toBe("3"));
  test("-2.5 -> -3", () => expect(rneg25).toBe("-3"));
  test("0.125 -> 0.13", () => expect(r0125).toBe("0.13"));
  test("2.55 -> 2.5 (f64 real)", () => expect(r255).toBe("2.5"));
  test("1.45 -> 1.4 (f64 real)", () => expect(r145).toBe("1.4"));
  test("-0.001 -> -0.00 (sinal preservado)", () => expect(negZero).toBe("-0.00"));
  test("1.005 -> 1.00 (f64 real)", () => expect(r1005).toBe("1.00"));
  test("1234.5678 -> 1234.57", () => expect(big).toBe("1234.57"));
  test("100 -> 100.00", () => expect(padZeros).toBe("100.00"));
});
