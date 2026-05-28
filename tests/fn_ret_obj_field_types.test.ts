import { describe, test, expect } from "rts:test";

// `const r = mk()` onde mk retorna object literal deve propagar os tipos
// de campo. Sem isso, `r.label + r.label` (string) virava soma numerica.
function mk(): { label: string } { return { label: "tag" }; }
const r = mk();
const concat = r.label + r.label;

// Campo numerico permanece soma.
function mn(): { n: number } { return { n: 5 }; }
const rn = mn();
const sum = rn.n + rn.n;

// Campos mistos string+number.
function mm(): { s: string; c: number } { return { s: "hi", c: 3 }; }
const rm = mm();
const mixed = rm.s + rm.c;

describe("fn return object field types", () => {
  test("campo string concatena", () => expect(concat).toBe("tagtag"));
  test("campo numerico soma", () => expect(sum).toBe(10));
  test("campos mistos", () => expect(mixed).toBe("hi3"));
});
