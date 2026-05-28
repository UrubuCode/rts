import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): find/findIndex/some/every cujo predicate captura
// var local davam lixo — o pass roteava map/filter/forEach/reduce ao caminho
// BOUND mas deixava find/some/every com fn_ptr nu (sem bound_args), entao a
// captura recebia o indice. Fix: variantes PARALLEL_{FIND,FIND_INDEX,SOME,
// EVERY}_BOUND que invocam via invoke_array_callback com bound_args.

let out = "";
function print(v: string): void { out += v + "\n"; }

function findFirst(arr: number[], target: number): number {
  return arr.find(x => x === target) ?? -1;
}
print(findFirst([10, 20, 30], 20) + "");   // 20

function findIdx(arr: number[], t: number): number {
  return arr.findIndex(x => x === t);
}
print(findIdx([10, 20, 30], 30) + "");     // 2

function allAbove(arr: number[], min: number): boolean {
  return arr.every(x => x >= min);
}
print(allAbove([5, 6, 7], 5) + "");        // true
print(allAbove([5, 6, 7], 6) + "");        // false

function someBelow(arr: number[], max: number): boolean {
  return arr.some(x => x < max);
}
print(someBelow([5, 6, 7], 6) + "");       // true
print(someBelow([5, 6, 7], 4) + "");       // false

// filter/map com captura continuam OK (regressao guard)
function above(arr: number[], lim: number): number[] {
  return arr.filter(x => x > lim);
}
print(above([1, 5, 2, 8], 4).join(","));   // 5,8

describe("array predicate captura local", () => {
  test("find/findIndex/some/every com captura", () =>
    expect(out).toBe("20\n2\ntrue\nfalse\ntrue\nfalse\n5,8\n"));
});
