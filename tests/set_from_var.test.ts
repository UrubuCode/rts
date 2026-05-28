import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `new Set(arr)` onde arr eh var/param (nao literal)
// criava um Set VAZIO — so' `new Set([1,2,3])` literal populava. Fix: runtime
// SET_FROM_VEC, roteado quando o arg eh Ident/Member (Vec ja' materializado).
// Espelha o MAP_FROM_ENTRIES do Map.

let out = "";
function print(v: string): void { out += v + "\n"; }

// dedup via Set(param)
function dedup(arr: number[]): number[] {
  return [...new Set(arr)];
}
print(dedup([1, 2, 2, 3, 3, 3]).join(","));   // 1,2,3

// Set(param) + has em filter
function intersect(a: number[], b: number[]): number[] {
  const setB = new Set(b);
  return a.filter(x => setB.has(x));
}
print(intersect([1, 2, 3, 4], [2, 4, 6]).join(","));  // 2,4

// Set de strings via var
const words = ["a", "b", "a", "c", "b"];
const us = new Set(words);
print(us.size + "");                          // 3
print(us.has("a") + "");                      // true
print(us.has("z") + "");                      // false

// Set(literal) continua OK (guard)
print([...new Set([5, 5, 6])].join(","));     // 5,6

describe("new Set(var/param)", () => {
  test("Set populado a partir de var/param", () =>
    expect(out).toBe("1,2,3\n2,4\n3\ntrue\nfalse\n5,6\n"));
});
