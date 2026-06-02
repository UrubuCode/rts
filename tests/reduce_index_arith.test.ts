import { describe, test, expect } from "rts:test";

// (#345) Callback de array method com o parametro INDICE usado em aritmetica
// dava lixo (-3.26e-322). Causa: double-lifting — a arrow de 3 params era
// liftada por lift_inline_arrows (__lifted_arr_method_reduce_N, 3 params) E
// re-liftada por this_arrow (__lifted_arrow_N, 2 params), que passava o idx
// como undefined sentinel (i64::MIN+2) e criava recursao mutua tanglada. Fix:
// this_arrow nao re-lifta idents ja' liftados internamente (__lifted_arr_method_*
// / __lifted_cap_*); parallel.* os chama direto com a aridade correta.

const arr = [10, 20, 30];

// reduce com idx usado em aritmetica
const r3 = arr.reduce((acc, x, i) => acc + x + i, 0); // 60 + (0+1+2) = 63
const rIdxOnly = arr.reduce((acc, x, i) => acc + i, 0); // 0+1+2 = 3
const rIdxMul = arr.reduce((acc, x, i) => acc + i * 2, 0); // 0+2+4 = 6

// reduce sem idx continua funcionando (nao-regressao)
const r2 = arr.reduce((acc, x) => acc + x, 0); // 60

// idx declarado mas nao usado (nao-regressao)
const rUnused = arr.reduce((acc, x, i) => acc + x, 0); // 60

// map com idx em aritmetica
const mapped = arr.map((x, i) => x + i).join(","); // 10,21,32

// forEach com idx acumulando
let feSum = 0;
arr.forEach((x, i) => { feSum = feSum + x + i; }); // 63

describe("array method callback com idx em aritmetica (#345)", () => {
  test("reduce acc+x+i", () => expect(`${r3}`).toBe("63"));
  test("reduce so idx", () => expect(`${rIdxOnly}`).toBe("3"));
  test("reduce idx*2", () => expect(`${rIdxMul}`).toBe("6"));
  test("reduce sem idx (nao-regressao)", () => expect(`${r2}`).toBe("60"));
  test("reduce idx nao usado (nao-regressao)", () => expect(`${rUnused}`).toBe("60"));
  test("map x+i", () => expect(mapped).toBe("10,21,32"));
  test("forEach acumulando x+i", () => expect(`${feSum}`).toBe("63"));
});
