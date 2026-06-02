import { describe, test, expect } from "rts:test";

// (#305) `1000000 * 1000000` dava -727379968 (overflow i32): literais inteiros
// que cabem em i32 eram classificados I32 e o `imul` operava em 32 bits,
// estourando antes de promover. Em JS `number` eh f64 (exato ate 2^53). Fix:
// `lower_mul` promove `i32 * i32` para i64 (cobre ate ~3*10^18) sem o custo de
// f64 em loops hot. add/sub/peephole `x*2^k` ficam inalterados.

// literal * literal
const litMul = 1000000 * 1000000; // 10^12

// var * var
const a = 100000;
const b = 100000;
const varMul = a * b; // 10^10

// literais com produto > 2^31
const wide = 123456 * 654321; // 80779853376

// acumulador em loop ultrapassando i32
let acc = 2;
for (let i = 0; i < 10; i++) {
  acc = acc * 10;
}
// 2 * 10^10 = 20000000000

// produto que ainda cabe em i32 (nao-regressao: continua exato)
const small = 1000 * 1000; // 1000000

// area-like (caso cotidiano: medida * medida)
function area(w: number, h: number): number {
  return w * h;
}
const big = area(1000000, 1000000); // 10^12

// (#305 follow-up) add/sub tambem promovem i32 -> i64.
const addLit = 2000000000 + 2000000000; // 4*10^9 (> i32::MAX)
const c = 2000000000;
const d = 2000000000;
const addVar = c + d; // 4*10^9
const subNeg = 1000000000 - 3000000000; // -2*10^9
const mixed = (100000 * 100000) + (2000000000 + 2000000000); // 10^10 + 4*10^9
let counter = 2000000000;
counter = counter + 1;
counter = counter + 1; // 2000000002
const addSmall = 1000 + 2000; // 3000 (nao-regressao)

describe("multiplicacao inteira nao overflowa em i32 (#305)", () => {
  test("literal * literal 10^12", () => expect(`${litMul}`).toBe("1000000000000"));
  test("var * var 10^10", () => expect(`${varMul}`).toBe("10000000000"));
  test("produto largo > 2^31", () => expect(`${wide}`).toBe("80779853376"));
  test("acumulador em loop", () => expect(`${acc}`).toBe("20000000000"));
  test("produto pequeno continua exato", () => expect(`${small}`).toBe("1000000"));
  test("fn area 10^12", () => expect(`${big}`).toBe("1000000000000"));

  test("add literal > 2^31", () => expect(`${addLit}`).toBe("4000000000"));
  test("add var > 2^31", () => expect(`${addVar}`).toBe("4000000000"));
  test("sub negativo > -2^31", () => expect(`${subNeg}`).toBe("-2000000000"));
  test("mul + add aninhado", () => expect(`${mixed}`).toBe("14000000000"));
  test("contador cruza 2^31", () => expect(`${counter}`).toBe("2000000002"));
  test("add pequeno continua exato", () => expect(`${addSmall}`).toBe("3000"));
});
