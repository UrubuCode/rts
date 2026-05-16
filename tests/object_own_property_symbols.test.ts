import { describe, test, expect } from "rts:test";

// (#798) Object.getOwnPropertySymbols deve retornar handles de Symbol
// gravados como computed keys; Object.keys / getOwnPropertyNames devem
// filtrar essas entries `@@sym:<handle>` da repr interna.

const sym1 = Symbol("a");
const sym2 = Symbol("b");
const obj: any = {
  normal: 1,
  [sym1]: "value1",
  [sym2]: "value2",
};

const symbols: any = Object.getOwnPropertySymbols(obj);
const count = symbols.length;
const has1 = symbols.includes(sym1);
const has2 = symbols.includes(sym2);
const keysJoined = Object.keys(obj).join(",");
const namesJoined = Object.getOwnPropertyNames(obj).join(",");

describe("Object.getOwnPropertySymbols (#798)", () => {
  test("count == 2", () => expect(count).toBe(2));
  test("includes sym1", () => expect(has1).toBe(true));
  test("includes sym2", () => expect(has2).toBe(true));
  test("Object.keys nao vaza @@sym:", () => expect(keysJoined).toBe("normal"));
  test("Object.getOwnPropertyNames nao vaza @@sym:", () =>
    expect(namesJoined).toBe("normal"));
});
