import { describe, test, expect } from "rts:test";

// parseFloat parseia o maior prefixo numerico valido e ignora o resto
// (JS spec). Antes fazia parse estrito e retornava NaN com qualquer sufixo.
const suffix = parseFloat("3.14abc");
const unit = parseFloat("42px");
const padded = parseFloat("  10.5  ");
const exp = parseFloat("1e3xyz");
const nanIsNaN = Number.isNaN(parseFloat("abc"));
const pure = parseFloat("3.14");
const negExp = parseFloat("-2.5e-3");
const dotStart = parseFloat(".5");
const dotEnd = parseFloat("5.");
const twoDots = parseFloat("1.2.3");
const hex = parseFloat("0xFF");

describe("parseFloat prefix parsing", () => {
  test("sufixo alfabetico", () => expect(suffix).toBe(3.14));
  test("unidade px", () => expect(unit).toBe(42));
  test("whitespace ao redor", () => expect(padded).toBe(10.5));
  test("expoente com sufixo", () => expect(exp).toBe(1000));
  test("nao-numerico vira NaN", () => expect(nanIsNaN).toBe(true));
  test("numero puro", () => expect(pure).toBe(3.14));
  test("expoente negativo", () => expect(negExp).toBe(-0.0025));
  test("ponto inicial", () => expect(dotStart).toBe(0.5));
  test("ponto final", () => expect(dotEnd).toBe(5));
  test("dois pontos para no segundo", () => expect(twoDots).toBe(1.2));
  test("0xFF le so o 0", () => expect(hex).toBe(0));
});
