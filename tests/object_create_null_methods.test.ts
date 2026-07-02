import { describe, test, expect } from "rts:test";

// (#777) `Object.create(null)` cria um objeto SEM prototype: nenhum método
// universal (toString/valueOf/hasOwnProperty) existe nele. Em JS real a
// leitura de uma key ausente retorna `undefined` (o teste antigo esperava o
// sentinel `0` do motor velho — reescrito JS-fiel no motor novo).

const o = Object.create(null);
const ts = (o as any).toString;
const tsEqO = (ts as any) === (o as any);
const tsUndef = (ts as any) === undefined;
const noProp = (o as any).nonexistent;
const tsTypeOf = typeof (o as any).toString;

describe("Object.create(null) sem prototype methods (#777)", () => {
  test("o.toString nao retorna o proprio handle", () => expect(tsEqO).toBe(false));
  test("o.toString eh undefined (key ausente)", () => expect(tsUndef).toBe(true));
  test("o.nonexistent eh undefined", () => expect((noProp as any) === undefined).toBe(true));
  test("typeof o.toString eh 'undefined'", () => expect(tsTypeOf).toBe("undefined"));
});
