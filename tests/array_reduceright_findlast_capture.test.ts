import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): reduceRight/findLast/findLastIndex cujo callback
// captura var local davam lixo — esses metodos nao eram roteados ao caminho
// BOUND (so' map/filter/find/some/every estavam). Fix: variantes
// PARALLEL_{REDUCE_RIGHT,REDUCE_RIGHT_NO_INIT,FIND_LAST,FIND_LAST_INDEX}_BOUND;
// roteadas SO' quando o callback tem captura (`__lifted_cap_*`).
//
// Nota: o `?? -1` em find/findLast SEM match eh bug ortogonal (find retorna
// handle-string "undefined", nao o sentinel) — coberto por casos COM match.

let out = "";
function print(v: string): void { out += v + "\n"; }

// reduceRight com init capturando local
function joinR(arr: string[], sep: string): string {
  return arr.reduceRight((acc, x) => acc + sep + x, "");
}
print(joinR(["a", "b", "c"], "-"));        // -c-b-a

function sumR(arr: number[], base: number): number {
  return arr.reduceRight((acc, x) => acc + x + base, 0);
}
print(sumR([1, 2, 3], 100) + "");          // 306

// findLast capturando local (com match)
function lastDiv(arr: number[], m: number): number {
  const r = arr.findLast(x => x % m === 0);
  return r === undefined ? -1 : r;
}
print(lastDiv([2, 3, 4, 5, 6], 2) + "");   // 6
print(lastDiv([1, 3, 9, 5], 3) + "");      // 9

// findLastIndex capturando local
function lastDivIdx(arr: number[], m: number): number {
  return arr.findLastIndex(x => x % m === 0);
}
print(lastDivIdx([2, 3, 4, 5], 2) + "");   // 2

describe("reduceRight/findLast captura local", () => {
  test("callback capturando local roteia ao BOUND", () =>
    expect(out).toBe("-c-b-a\n306\n6\n9\n2\n"));
});
