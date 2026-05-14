import { describe, test, expect } from "rts:test";

// new Array(N) preenche slots com undefined sentinel (JS spec)
const a1 = new Array(5);
const a1_len = a1.length;
const a1_first_undef = a1[0] === undefined;
const a1_last_undef = a1[4] === undefined;
// OOB read returns "" string handle (sem ser sentinel undefined) — fora
// do escopo de #786, cobrir em follow-up.

// === undefined / !== undefined em slot vazio
const a2 = new Array(3);
const a2_neq = a2[1] !== undefined;

// Array(N) sem new
const a3 = Array(3);
const a3_len = a3.length;
const a3_first_undef = a3[0] === undefined;

// Multi-arg constructor preserva valores (nao zera)
const a4 = new Array(1, 2, 3);
const a4_values = a4.join(",");

// new Array() (0 args)
const a5 = new Array();
const a5_len = a5.length;

describe("array_constructor (#786)", () => {
  test("new Array(5).length === 5", () => expect(a1_len).toBe(5));
  test("new Array(5)[0] === undefined", () => expect(a1_first_undef).toBe(true));
  test("new Array(5)[4] === undefined", () => expect(a1_last_undef).toBe(true));
  test("slot !== undefined comparison", () => expect(a2_neq).toBe(false));
  test("Array(3) sem new tambem zera com undefined", () => expect(a3_first_undef).toBe(true));
  test("Array(3).length === 3", () => expect(a3_len).toBe(3));
  test("new Array(1,2,3).join", () => expect(a4_values).toBe("1,2,3"));
  test("new Array() vazio", () => expect(a5_len).toBe(0));
});
