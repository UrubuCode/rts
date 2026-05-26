import { describe, test, expect } from "rts:test";

// Regression (#195 parcial): arrow liftada (em const/return dentro de fn)
// deve PRESERVAR seus próprios parâmetros. Antes, lift_arrow_to_ident
// descartava os params (`parameters: Vec::new()`), então `(i) => i*2`
// virava uma fn sem `i` → "undefined variable i".

// arrow com 1 param, chamada direta
const dbl = (x: number) => x * 2;

// arrow com 2 params
const add = (a: number, b: number) => a + b;

// arrow com param dentro de fn, chamada local
function useLocal(): number {
  const g = (i: number) => i + 100;
  return g(5);
}

// arrow com destructuring de param
const pt = ({ x, y }: { x: number; y: number }) => x + y;

// arrow com param default
const inc = (n: number, by = 1) => n + by;

describe("lifted arrow params", () => {
  test("arrow 1 param", () => expect(dbl(21)).toBe(42));
  test("arrow 2 params", () => expect(add(20, 22)).toBe(42));
  test("arrow param dentro de fn", () => expect(useLocal()).toBe(105));
  test("arrow destructuring de param", () => expect(pt({ x: 40, y: 2 })).toBe(42));
  test("arrow param com default (fornecido)", () => expect(inc(40, 2)).toBe(42));
  test("arrow param com default (omitido)", () => expect(inc(41)).toBe(42));
});
