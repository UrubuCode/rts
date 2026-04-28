// Reduce silent (#247 D): padroes `let acc = init; for (x of arr) acc OP= ...;`
// sao detectados pelo reduce_pass e reescritos para parallel.reduce.
//
// Testes verificam correctness (paralelo nao deve mudar o resultado de
// operacoes associativas). Se o pass nao detectasse, o for...of cairia
// em outra rota (purity_pass for_each ou serial) — em ambos casos
// produziria o mesmo resultado, entao falha so seria sintoma de bug
// de codegen, nao de paralelismo.

import { describe, test, expect } from "rts:test";
import { gc, math } from "rts";

const arr = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

// SUM via `acc = acc + x`
let sum1 = 0;
for (const x of arr) {
  sum1 = sum1 + x;
}

// SUM via `+=`
let sum2 = 0;
for (const x of arr) {
  sum2 += x;
}

// PRODUTO via `*=`
let prod = 1;
for (const x of arr) {
  prod *= x;
}

// SUM via fn pura (math.abs_i64)
let sumAbs = 0;
for (const x of arr) {
  sumAbs = sumAbs + math.abs_i64(x);
}

describe("fixture:parallel_reduce_silent", () => {
  test("sum via assignment combine", () => {
    expect(gc.string_from_i64(sum1)).toBe("55");
  });

  test("sum via +=", () => {
    expect(gc.string_from_i64(sum2)).toBe("55");
  });

  test("produto via *=", () => {
    expect(gc.string_from_i64(prod)).toBe("3628800");
  });

  test("sum com fn pura no body (math.abs_i64)", () => {
    expect(gc.string_from_i64(sumAbs)).toBe("55");
  });
});
