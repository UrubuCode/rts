import { describe, test, expect } from "rts:test";

// (#753) Symbol pode ser usado como key em obj literal computed,
// computed assignment e como rhs de `in`. Ate antes do fix, o codegen
// resolvia symbol via STRING_PTR/LEN (que falha em handles nao-string),
// e `in` so' suportava Vec.

const sym = Symbol("test");
const obj: any = {};
obj[sym] = "value";
const hasViaIn = sym in obj;
const valViaGet = obj[sym];

const withSym = Object.assign({}, { [sym]: "ol", normal: "data" });
const hasInAssigned = sym in withSym;
const normalInMap = "normal" in withSym;
const missingKey = "nope" in withSym;

describe("Symbol as object key (#753)", () => {
  test("obj[sym] = v + obj[sym] read", () => expect(valViaGet).toBe("value"));
  test("sym in obj", () => expect(hasViaIn).toBe(true));
  test("Object.assign preserva [sym] computed key", () => expect(hasInAssigned).toBe(true));
  test("string key in Map handle (regressao geral do in)", () => expect(normalInMap).toBe(true));
  test("ausente in Map retorna false", () => expect(missingKey).toBe(false));
});
