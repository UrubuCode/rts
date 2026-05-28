import { describe, test, expect } from "rts:test";

// Object.assign deve propagar tipos de campo dos sources (igual ao spread).
// Sem isso, `m.a + m.b` (strings) virava soma numerica dos handles.
const merged = Object.assign({}, { a: "x" }, { b: "y" });
const concat = merged.a + merged.b;

// Source posterior sobrescreve.
const o2 = Object.assign({}, { k: "first" }, { k: "second" });
const ov = o2.k;

// Numerico permanece soma.
const n = Object.assign({}, { a: 1 }, { b: 2 });
const sum = n.a + n.b;

// Assign de ident registrado.
const base = { name: "n", val: "v" };
const m3 = Object.assign({}, base);
const fromIdent = m3.name + m3.val;

describe("Object.assign field types", () => {
  test("campos string concatenam", () => expect(concat).toBe("xy"));
  test("source posterior sobrescreve", () => expect(ov).toBe("second"));
  test("numerico soma", () => expect(sum).toBe(3));
  test("assign de ident preserva tipos", () => expect(fromIdent).toBe("nv"));
});
