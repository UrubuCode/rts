import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): Map/Set passado como PARAMETRO -> `.size` dava 0.
// `.values()`/`.has` funcionavam mas `.size` caia em MAP_GET("size")=0 pois o
// param nao era marcado local_map_vars. Fix: param `: Map/Set/Weak*` marca
// local_map_vars em user_fn.rs (mesma rota do retorno #1260).

let out = "";
function print(v: string): void { out += v + "\n"; }

function countEntries(m: Map<string, number>): number {
  return m.size;
}
const mm = new Map<string, number>([["a", 1], ["b", 2], ["c", 3]]);
print(countEntries(mm) + "");          // 3

function setSize(s: Set<number>): number {
  return s.size;
}
print(setSize(new Set([1, 2, 3, 4])) + "");  // 4

// param Map iterando values continua OK
function sumValues(m: Map<string, number>): number {
  let total = 0;
  for (const v of m.values()) total += v;
  return total;
}
print(sumValues(mm) + "");             // 6

// param Set + has
function hasIt(s: Set<number>, x: number): boolean {
  return s.has(x);
}
print(hasIt(new Set([5, 6]), 6) + ""); // true

describe("Map/Set size como parametro", () => {
  test("size funciona em Map/Set passado por param", () =>
    expect(out).toBe("3\n4\n6\ntrue\n"));
});
