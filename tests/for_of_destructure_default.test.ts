import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `for (const [a = 0, b = 0] of data)` (default no
// pattern de destructuring) dava erro "for-of destructuring suporta apenas
// idents simples ou elision" e nao iterava. Fix: aceita Pat::Assign extraindo
// o ident interno (slot ausente ja' vira sentinel 0 p/ number, cobrindo `= 0`).

let out = "";
function print(v: string): void { out += v + "\n"; }

const data: number[][] = [[1], [2, 3], []];
let r = "";
for (const [a = 0, b = 0] of data) r += "(" + a + "," + b + ")";
print(r);                       // (1,0)(2,3)(0,0)

// sem default: slot ausente é `undefined` (igual a JS/Node — `[a,b]` de `[1]`
// dá b===undefined). O motor velho usava o sentinel 0, divergente.
let r2 = "";
for (const [a, b] of data) r2 += a + ":" + b + " ";
print(r2.trim());               // 1:undefined 2:3 undefined:undefined

// pairs com default
const pairs: [string, number][] = [["a", 1], ["b", 2]];
let r3 = "";
for (const [k = "?", v = 0] of pairs) r3 += k + v;
print(r3);                      // a1b2

describe("for-of destructure default", () => {
  test("default no pattern nao bloqueia iteracao", () =>
    expect(out).toBe("(1,0)(2,3)(0,0)\n1:undefined 2:3 undefined:undefined\na1b2\n"));
});
