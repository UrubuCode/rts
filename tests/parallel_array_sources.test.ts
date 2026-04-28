// Pedaco E do epic #247: garante que parallel.* (e os passes silent
// que reescrevem pra parallel.*) aceitam arrays vindos de varias
// fontes alem de literais inline.
import { describe, test, expect } from "rts:test";
import { gc, collections, atomic, parallel } from "rts";

function double(x: i64): i64 { return x * 2; }
function add(acc: i64, x: i64): i64 { return acc + x; }

const counter = atomic.i64_new(0);
function tally(x: i64): void {
  atomic.i64_fetch_add(counter, x);
}

// Fonte 1: array em variavel local top-level
const arrVar = [1, 2, 3, 4, 5];

// Fonte 2: array retornado de fn
function makeArr(): i64 {
  return [10, 20, 30] as unknown as i64;
}

// Fonte 3: array literal inline (controle)
const sumLiteral = [100, 200, 300].reduce(add, 0);

// Aplicacoes via array methods (passe C reescreve pra parallel.*)
const doubledFromVar = arrVar.map(double);
const dvLen = collections.vec_len(doubledFromVar);
const dvFirst = collections.vec_get(doubledFromVar, 0);
const dvLast = collections.vec_get(doubledFromVar, 4);

const sumFromVar = arrVar.reduce(add, 0);

atomic.i64_store(counter, 0);
arrVar.forEach(tally);
const counterFromVar = atomic.i64_load(counter);

// Direto via parallel.* (sem array methods)
const directMap = parallel.map(arrVar, double as unknown as i64);
const dmLen = collections.vec_len(directMap);

describe("fixture:parallel_array_sources", () => {
  test("arr.map em variavel local funciona", () => {
    expect(gc.string_from_i64(dvLen)).toBe("5");
    expect(gc.string_from_i64(dvFirst)).toBe("2");
    expect(gc.string_from_i64(dvLast)).toBe("10");
  });

  test("arr.reduce em variavel local funciona", () => {
    expect(gc.string_from_i64(sumFromVar)).toBe("15");
  });

  test("arr.forEach em variavel local funciona", () => {
    expect(gc.string_from_i64(counterFromVar)).toBe("15");
  });

  test("array literal direto via reduce funciona (controle)", () => {
    expect(gc.string_from_i64(sumLiteral)).toBe("600");
  });

  test("parallel.map() chamado direto aceita variavel", () => {
    expect(gc.string_from_i64(dmLen)).toBe("5");
  });
});
