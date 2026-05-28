import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `[..].fill(0).map((_, i) => i)` chain direto com
// callback de 2 params (val, idx) CRASHAVA. O lift de arrow so' reconhecia
// receiver Array literal/Ident como array; receiver chain (.fill().map()) caia
// em arity=1 e o 2o param (idx) ficava indefinido -> crash. Fix: novo helper
// expr_is_array_returning_call reconhece chain de metodo array-returning.

let out = "";
function print(v: string): void { out += v + "\n"; }

const r = new Array(3).fill(0).map((_, i) => i);
print(r.join(","));              // 0,1,2

const r2 = [0, 0, 0].fill(0).map((_, i) => i);
print(r2.join(","));            // 0,1,2

const r3 = [1, 2, 3].fill(5).map((x, i) => x + i);
print(r3.join(","));            // 5,6,7

// chain map().filter() com idx
const r4 = [1, 2, 3, 4].map((x, i) => x + i).filter((x, i) => i % 2 === 0);
print(r4.join(","));            // 1,5  (map->[1,3,5,7], filter idx pares -> 1,5)

// guards: callback 1-param e via var continuam OK
const r5 = [1, 2, 3].fill(2).map(x => x * 10);
print(r5.join(","));            // 20,20,20

describe("array chain + callback com idx", () => {
  test("chain de metodo array-returning + arrow 2-param", () =>
    expect(out).toBe("0,1,2\n0,1,2\n5,6,7\n1,5\n20,20,20\n"));
});
