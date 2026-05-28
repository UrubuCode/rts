import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `Math.min(...arr)`/`Math.max(...arr)` com arr
// var/param dentro de fn davam lixo — o caso 1-arg (spread) caia em
// coerce_to_f64(arr_handle) (handle interpretado como f64) em vez de reduzir
// sobre o Vec. Standalone com const-array funcionava (spread expandido p/ N
// args). Fix: VEC_MIN/VEC_MAX no runtime; caso 1-arg-spread roteia p/ eles.

let out = "";
function print(v: string): void { out += v + "\n"; }

// param array
function minMax(arr: number[]): [number, number] {
  return [Math.min(...arr), Math.max(...arr)];
}
const r = minMax([3, 1, 4, 1, 5]);
print(r.join(","));              // 1,5

// const local dentro de fn
function f2(): number[] {
  const local = [7, 2, 9];
  return [Math.min(...local), Math.max(...local)];
}
print(f2().join(","));           // 2,9

// standalone top-level continua OK
const arr = [10, 5, 8];
print(Math.min(...arr) + "," + Math.max(...arr));  // 5,10

// Math.min/max literal (sem spread) continua OK
print(Math.min(3, 1) + "," + Math.max(3, 1));      // 1,3

describe("Math.min/max com spread", () => {
  test("spread de array var/param reduz corretamente", () =>
    expect(out).toBe("1,5\n2,9\n5,10\n1,3\n"));
});
