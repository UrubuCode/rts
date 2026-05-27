import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `return g(n-1).concat([n])` aplicava TCO
// (return_call) em g(n-1) e DESCARTAVA o .concat — o callee real do tail eh
// o method call .concat, nao g. Resultado: recursao retornando
// array.concat() dava vazio. Fix: is_direct_call_expr so' eh tail-call
// quando o callee eh Ident (chamada direta), nao Member (method call).

let out = "";
function print(v: string): void { out += v + "\n"; }

// quicksort recursivo (recursao + concat em tail)
function quicksort(arr: number[]): number[] {
  if (arr.length <= 1) return arr;
  const pivot = arr[0];
  const less: number[] = [];
  const more: number[] = [];
  for (let i = 1; i < arr.length; i++) {
    if (arr[i] < pivot) less.push(arr[i]); else more.push(arr[i]);
  }
  return quicksort(less).concat([pivot]).concat(quicksort(more));
}
print(quicksort([3, 1, 4, 1, 5, 9, 2, 6]).join(",")); // 1,1,2,3,4,5,6,9

// recursao retornando array.concat
function build(n: number): number[] {
  if (n <= 0) return [];
  return build(n - 1).concat([n]);
}
print(build(4).join(","));  // 1,2,3,4

// TCO genuino (tail-call direto) preservado — sem stack overflow
function countdown(n: number): number {
  if (n <= 0) return 0;
  return countdown(n - 1);
}
print("cd=" + countdown(50000));  // 0

describe("tco method chain not tail", () => {
  test("recursao + method chain nao descarta o chain", () =>
    expect(out).toBe("1,1,2,3,4,5,6,9\n1,2,3,4\ncd=0\n"));
});
