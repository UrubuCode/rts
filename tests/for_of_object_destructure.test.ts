import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `for (const {a, b} of items)` (object pattern no
// bind do for-of) dava "for-of bind deve ser ident ou array pattern" e nao
// iterava. Fix: coleta (var, key) do object pattern e extrai via MAP_GET por
// chave no body (analogo ao array pattern via vec_get por indice).

let out = "";
function print(v: string): void { out += v + "\n"; }

const items = [{ id: 1, n: "a" }, { id: 2, n: "b" }, { id: 3, n: "c" }];

// shorthand {id, n}
let r1 = "";
for (const { id, n } of items) r1 += id + n;
print(r1);                       // 1a2b3c

// rename {id: x}
let r2 = "";
for (const { id: x } of items) r2 += x;
print(r2);                       // 123

// campo nested acessado depois
const data = [{ pos: { x: 1, y: 2 } }, { pos: { x: 3, y: 4 } }];
let r3 = "";
for (const { pos } of data) r3 += pos.x + "/" + pos.y + " ";
print(r3.trim());                // 1/2 3/4

// soma de campo via destructuring
const txs = [{ amount: 100 }, { amount: 50 }, { amount: 25 }];
let total = 0;
for (const { amount } of txs) total += amount;
print(total + "");               // 175

// array pattern continua OK (guard)
const pairs: [string, number][] = [["a", 1], ["b", 2]];
let r5 = "";
for (const [k, v] of pairs) r5 += k + v;
print(r5);                       // a1b2

describe("for-of object destructuring", () => {
  test("object pattern no bind do for-of", () =>
    expect(out).toBe("1a2b3c\n123\n1/2 3/4\n175\na1b2\n"));
});
