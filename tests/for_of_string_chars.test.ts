import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `for (const c of "abc")` / string var nao
// iterava os chars — o for-of tratava a string como Vec (VEC_LEN=-1) e o
// loop nao rodava. Fix: detecta iteravel string (literal/template/var
// rastreada) e converte p/ Vec de char-handles via SPREAD_INTO_VEC (mesmo
// helper do spread `[..."abc"]`).

let out = "";
function print(v: string): void { out += v + "\n"; }

// literal
let lit = "";
for (const c of "abc") lit += c + ".";
print(lit);                       // a.b.c.

// var
const s = "hello";
let v = "";
for (const c of s) v += c;
print(v);                         // hello

// var anotada
const w: string = "xyz";
let parts = "";
for (const c of w) parts += c.toUpperCase();
print(parts);                     // XYZ

// template literal
const name = "rts";
let t = "";
for (const c of `${name}!`) t += c;
print(t);                         // rts!

describe("for-of string chars", () => {
  test("itera codepoints da string", () =>
    expect(out).toBe("a.b.c.\nhello\nXYZ\nrts!\n"));
});
