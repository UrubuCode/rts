import { describe, test, expect } from "rts:test";

// Unary minus de string deve coagir para numero (-ToNumber). Antes negava
// o handle cru. `-"abc"` = NaN.
const m5 = -"5";
const m314 = -"3.14";
const s = "42";
const mvar = -s;
const mpad = -"  10  ";
const x = "100";
const arith = -x + 1;
const abcIsNaN = Number.isNaN(-"abc");

// Minus de numero nao regride.
const n = 7;
const mnum = -n;
const mexpr = -(3 + 2);

describe("unary minus string", () => {
  test('-"5" -> -5', () => expect(m5).toBe(-5));
  test('-"3.14" -> -3.14', () => expect(m314).toBe(-3.14));
  test("-var string", () => expect(mvar).toBe(-42));
  test("-string com espacos", () => expect(mpad).toBe(-10));
  test("-x + 1 aritmetica", () => expect(arith).toBe(-99));
  test('-"abc" eh NaN', () => expect(abcIsNaN).toBe(true));
  test("-num intacto", () => expect(mnum).toBe(-7));
  test("-(expr) intacto", () => expect(mexpr).toBe(-5));
});
