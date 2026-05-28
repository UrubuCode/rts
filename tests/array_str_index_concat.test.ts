import { describe, test, expect } from "rts:test";

// `arr[i] + arr[j]` de array de strings deve concatenar, nao somar os
// handles. Ambos operandos sao I64-ambiguos (vec_get) — ADD_AUTO decide
// em runtime: concat se string, soma se numero.
const strs = ["aa", "bb", "cc"];
const concatLit = strs[0] + strs[1];

const xs: string[] = ["x", "y"];
const concatTyped = xs[0] + xs[1];

const parts = "a-b-c".split("-");
const concatSplit = parts[0] + parts[2];

// Object.entries [key,val] de strings.
const ent = Object.entries({ k: "val" });
const concatEntries = ent[0][0] + ent[0][1];

// Arrays numericos continuam somando (sem regressao).
const nums = [10, 20, 30];
const sumLit = nums[0] + nums[1];
const ns2: number[] = [1, 2];
const sumTyped = ns2[0] + ns2[1];

// Misto string + number.
const mixed = xs[0] + nums[0];

describe("array string index concat", () => {
  test("array literal strings concatena", () => expect(concatLit).toBe("aabb"));
  test("string[] tipado concatena", () => expect(concatTyped).toBe("xy"));
  test("split result concatena", () => expect(concatSplit).toBe("ac"));
  test("Object.entries tupla concatena", () => expect(concatEntries).toBe("kval"));
  test("array numerico literal soma", () => expect(sumLit).toBe(30));
  test("number[] tipado soma", () => expect(sumTyped).toBe(3));
  test("misto string+number", () => expect(mixed).toBe("x10"));
});
