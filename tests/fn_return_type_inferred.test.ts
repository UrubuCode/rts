import { describe, test, expect } from "rts:test";

// (codegen) Fn sem anotacao explicita de return type mas com `return <expr>`
// no body retornava void (codegen emit `function () tail` sem retorno) e
// descartava o valor — `mul(3, 4)` retornava 0.
// Fix: inferir F64 (number) como default quando o body tem return value.

function mul(a: number, b: number) {
  return a * b;
}

function add(a: number, b: number) {
  return a + b;
}

function maxOf(a: number, b: number) {
  if (a > b) return a;
  return b;
}

function noReturn(x: number) {
  // sem return -> continua void
  const _ = x + 1;
}

describe("inference de return type quando ausente", () => {
  test("mul(3,4) = 12", () => expect(mul(3, 4)).toBe(12));
  test("add(2,5) = 7", () => expect(add(2, 5)).toBe(7));
  test("maxOf(10, 7) = 10", () => expect(maxOf(10, 7)).toBe(10));
  test("maxOf(3, 9) = 9 (branch else)", () => expect(maxOf(3, 9)).toBe(9));
  test("noReturn(x) sem regressao", () => {
    noReturn(1);
    expect(1).toBe(1);
  });
});
