import { describe, test, expect } from "rts:test";

// Conformidade JS: bitwise/shift sobre VARIÁVEL aplicam ToInt32 (32-bit wrap
// com sinal) no resultado, como o JS exige. Antes, `s << 13` sobre variável
// dava resultado 64-bit em vez do int32 envolvido (literais const-folded já
// funcionavam; o bug era só sobre operandos dinâmicos).
//
// Pré-computado no top-level (valores em const) — ver nota CLAUDE.md sobre GC
// em test() closures.

let s: number = 123456789;
const shl13 = s << 13;        // 2040700928
const shl5 = s << 5;          // -344350048 (wrap de sinal!)

let big: number = 3000000000; // > 2^31
const bigShl1 = big << 1;     // 1705032704 (ToInt32(big) primeiro)
const bigOr0 = big | 0;       // -1294967296 (ToInt32)

let x: number = 1;
const x31 = x << 31;          // -2147483648 (bit de sinal)

let neg: number = -5;
const negShr = neg >> 1;      // -3 (arithmetic, preserva sinal)

const andMask = s & 0xFFFF;   // 52501

// xorshift32 — a sequência completa deve casar com o JS real.
let seed: number = 123456789;
function rnd(): number {
  seed = seed ^ (seed << 13);
  seed = seed ^ (seed >>> 17);
  seed = seed ^ (seed << 5);
  return (seed >>> 0) % 1000000;
}
const r0 = rnd(); // 967881
const r1 = rnd(); // 813396
const r2 = rnd(); // 77441

describe("bitwise/shift int32 conformance", () => {
  test("<< trunca para int32", () => {
    expect(shl13).toBe(2040700928);
  });
  test("<< envolve para negativo (wrap de sinal)", () => {
    expect(shl5).toBe(-344350048);
  });
  test("ToInt32 de operando > 2^31 antes do shift", () => {
    expect(bigShl1).toBe(1705032704);
  });
  test("| 0 aplica ToInt32", () => {
    expect(bigOr0).toBe(-1294967296);
  });
  test("1 << 31 = bit de sinal", () => {
    expect(x31).toBe(-2147483648);
  });
  test(">> arithmetic preserva sinal de negativo", () => {
    expect(negShr).toBe(-3);
  });
  test("& de valor pequeno inalterado", () => {
    expect(andMask).toBe(52501);
  });
  test("xorshift32 sequência casa com JS", () => {
    expect(r0).toBe(967881);
    expect(r1).toBe(813396);
    expect(r2).toBe(77441);
  });
});
