import { describe, test, expect } from "rts:test";

// Regression (cross-runtime #316): `[...set]` / Array.from(set) repetia o
// primeiro elemento N vezes (`[1,1,1]`). Causa: SPREAD_INTO_VEC tratava Set
// como Map e iterava m.values() (dummy 1), nao as keys (elementos reais do
// Set, storage interno Map<keyStr,1>). Fix: detecta Set e extrai elementos
// via MAP_VALUES (mesma conversao key->valor).

let out = "";
function print(v: string): void { out += v + "\n"; }

print([...new Set([1, 2, 3])].join(","));         // 1,2,3
print(Array.from(new Set([5, 5, 6])).join(","));  // 5,6 (dedup numerico)
print([...new Set(["x", "y", "z"])].join(","));   // x,y,z
// spread misto com Set
print([0, ...new Set([1, 2]), 3].join(","));      // 0,1,2,3

describe("set spread", () => {
  test("itera elementos reais (nao repete o 1o)", () =>
    expect(out).toBe("1,2,3\n5,6\nx,y,z\n0,1,2,3\n"));
});
