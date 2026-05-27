import { describe, test, expect } from "rts:test";

// `obj?.f()` (optional chaining method call) cujo metodo retorna string:
// o resultado eh handle de string e deve formatar como texto, nao numero
// cru. Antes o merge do optchain nao propagava a ambiguidade do retorno.
const obj = { f(): string { return "ok"; } };
const direct = "" + obj?.f();
const viaVar = obj?.f();
const concat = "r=" + viaVar;

// Metodo numerico via optchain permanece numero.
const o2 = { g(): number { return 42; } };
const numV = o2?.g();
const numArith = (o2?.g() ?? 0) + 1;

// Optchain em null -> undefined.
const o3: { f(): string } | null = null;
const nullRes = "" + o3?.f();

describe("optchain method string return", () => {
  test("obj?.f() string formata como texto", () => expect(direct).toBe("ok"));
  test("via var concatena texto", () => expect(concat).toBe("r=ok"));
  test("metodo numerico via optchain", () => expect(numV).toBe(42));
  test("aritmetica intacta", () => expect(numArith).toBe(43));
  test("optchain em null vira undefined", () => expect(nullRes).toBe("undefined"));
});
