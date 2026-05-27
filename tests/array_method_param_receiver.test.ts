import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// (cross-runtime #1065) arr.map/filter/reduce sobre param/local array DENTRO
// de user fn. Antes do fix, o receiver param/local nao era reconhecido como
// array (so' idents top-level eram), o `.map(arrow)` nao virava parallel.map
// e o codegen trapava (SIGILL). O arrow nao captura locals (so' literais).

// param `number[]`
function doubleAll(xs: number[]): number[] {
  return xs.map(x => x * 2);
}
const a = doubleAll([1, 2, 3]).join(",");

// param + filter
function evens(xs: number[]): number[] {
  return xs.filter(x => x % 2 === 0);
}
const b = evens([1, 2, 3, 4, 5, 6]).join(",");

// local anotado array
function localArr(): string {
  const ns: number[] = [10, 20, 30];
  return ns.map(n => n + 1).join(",");
}
const c = localArr();

// reduce sobre param
function sum(xs: number[]): number {
  return xs.reduce((acc, x) => acc + x, 0);
}
const d = sum([1, 2, 3, 4]);

describe("array method over param/local receiver (#1065)", () => {
  test("map over param number[]", () => expect(a).toBe("2,4,6"));
  test("filter over param number[]", () => expect(b).toBe("2,4,6"));
  test("map over local array", () => expect(c).toBe("11,21,31"));
  test("reduce over param", () => expect(`${d}`).toBe("10"));
});
