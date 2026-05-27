import { describe, test, expect } from "rts:test";

// (cross-runtime #345) callback de Array.prototype.reduce recebe
// (accumulator, currentValue, currentIndex). Antes do fix, o lift de arrow
// para reduce fixava arrow_arity=2, entao arrows de 3 params `(acc, x, i)`
// nao eram liftados ("unsupported call expression form") e o runtime
// PARALLEL_REDUCE chamava o callback com so' 2 args. Agora arrow_arity=3 e
// PARALLEL_REDUCE/_NO_INIT passam o index como 3o arg.

// soma usando index
function weightedSum(): number {
  return [10, 20, 30].reduce((acc, x, i) => acc + x * i, 0); // 0 + 20 + 60 = 80
}

// index-only
function indexSum(): number {
  return [0, 0, 0, 0].reduce((acc, x, i) => acc + i, 0); // 0+1+2+3 = 6
}

// string concat com index
function tagged(): string {
  return ["a", "b", "c"].reduce((acc, str, i) => acc + i + str, ""); // "0a1b2c"
}

// reduce sem init com index (callback roda a partir do indice 1)
function noInit(): number {
  return [5, 10, 20].reduce((acc, x, i) => acc + x + i); // 5 + (10+1) + (20+2) = 38
}

// callback de 2 params continua funcionando (arity menor que 3)
function plain(): number {
  return [1, 2, 3, 4].reduce((acc, x) => acc + x, 0); // 10
}

const a = weightedSum();
const b = indexSum();
const c = tagged();
const d = noInit();
const e = plain();

describe("reduce with index param (#345)", () => {
  test("weighted sum acc+x*i", () => expect(`${a}`).toBe("80"));
  test("index-only sum", () => expect(`${b}`).toBe("6"));
  test("string concat with index", () => expect(c).toBe("0a1b2c"));
  test("reduce no-init with index", () => expect(`${d}`).toBe("38"));
  test("2-param reduce still works", () => expect(`${e}`).toBe("10"));
});
