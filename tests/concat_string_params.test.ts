import { describe, test, expect } from "rts:test";

// `(a: string, b: string) => a + b` deve inferir retorno string a partir
// dos tipos dos params. Antes o concat virava i64 (handle cru).
const arrow2 = (a: string, b: string) => a + b;
const r1 = arrow2("x", "y");

function fn2(a: string, b: string) { return a + b; }
const r2 = fn2("p", "q");

const id = (s: string) => s;
const r3 = id("hi") + "!";

const cat3 = (a: string, b: string, c: string) => a + b + c;
const r4 = cat3("1", "2", "3");

// Params numericos permanecem soma (sem regressao).
const num2 = (a: number, b: number) => a + b;
const n1 = num2(3, 4) + 10;

describe("concat string params", () => {
  test("arrow 2 params string", () => expect(r1).toBe("xy"));
  test("fn nomeada sem anotacao retorno", () => expect(r2).toBe("pq"));
  test("identidade string", () => expect(r3).toBe("hi!"));
  test("3 params string", () => expect(r4).toBe("123"));
  test("params number somam", () => expect(n1).toBe(17));
});
