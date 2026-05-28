import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `for (const x of setNumerico) sum += x` somava
// o HANDLE da key string (lixo) — o for-of de Set usava MAP_KEYS (keys string
// cruas) em vez de MAP_VALUES (que reconverte key -> number via parse). Set de
// strings funcionava (key ja' eh string). Fix: usar MAP_VALUES (mesma
// conversao do spread de Set #1229).

let out = "";
function print(v: string): void { out += v + "\n"; }

// Set numerico: soma
const s = new Set([1, 2, 3, 4]);
let sum = 0;
for (const x of s) sum += x;
print(sum + "");                  // 10

// Set numerico: multiplicacao
let prod = 1;
for (const x of new Set([2, 3, 5])) prod *= x;
print(prod + "");                 // 30

// Set de strings preservado
const ss = new Set(["a", "b", "c"]);
let r = "";
for (const x of ss) r += x;
print(r);                         // abc

// dedup + soma (Set de duplicatas)
let s2 = 0;
for (const x of new Set([1, 1, 2, 2, 3])) s2 += x;
print(s2 + "");                   // 6

describe("for-of set numeric", () => {
  test("Set numerico soma os valores, nao handles", () =>
    expect(out).toBe("10\n30\nabc\n6\n"));
});
