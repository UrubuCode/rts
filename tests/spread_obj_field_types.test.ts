import { describe, test, expect } from "rts:test";

// Objeto via spread `{...base}` deve herdar os tipos de campo do fonte.
// Sem isso, `ext.name + ext.val` (ambos string) virava soma numerica dos
// handles em vez de concatenacao.
const base = { name: "n", val: "v" };
const ext = { ...base, extra: "e" };
const concat3 = ext.name + ext.val + ext.extra;

// Prop explicita depois do spread sobrescreve.
const o2 = { ...base, name: "over" };
const overrideConcat = o2.name + o2.val;

// Spread numerico permanece soma.
const nums = { a: 1, b: 2 };
const en = { ...nums };
const sum = en.a + en.b;

// Spread de objeto que ja veio de spread.
const e3 = { ...ext };
const reSpread = e3.name + e3.val;

describe("spread object field types", () => {
  test("campos string do spread concatenam", () => expect(concat3).toBe("nve"));
  test("prop explicita sobrescreve spread", () => expect(overrideConcat).toBe("overv"));
  test("spread numerico soma", () => expect(sum).toBe(3));
  test("spread de spread preserva tipos", () => expect(reSpread).toBe("nv"));
});
