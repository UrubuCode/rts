import { describe, test, expect } from "rts:test";

// Regression (cross-runtime #1275 parte 2): float fracionario literal em push,
// object literal e Map.set era truncado. Fix: armazena bits f64 (helper
// expr_is_frac_float_lit centralizado); leitura (index/join/member/Map.get +
// template) reinterpreta via heuristica >2^53. Int e F64 inteiro-valued seguem
// i64 (sem regredir destructuring/aritmetica de slot).

let out = "";
function print(v: string): void { out += v + "\n"; }

// push de float
const arr: number[] = [];
arr.push(1.5);
arr.push(2.5);
arr.push(3.0);
print(arr[0] + "");              // 1.5
print(arr.join(","));            // 1.5,2.5,3

// object literal float
const o = { price: 9.99, qty: 3 };
print(o.price + "");             // 9.99
print(o.qty + "");               // 3

// Map.set float
const m = new Map<string, number>();
m.set("pi", 3.14);
m.set("n", 42);
print(m.get("pi") + "");         // 3.14
print(m.get("n") + "");          // 42

// guards: int em push/Map nao regride
const ai: number[] = [];
ai.push(10);
ai.push(20);
print(ai.join(","));             // 10,20

describe("float em collection storage (#1275 parte 2)", () => {
  test("push/object/Map.set de float preservam valor; int intacto", () =>
    expect(out).toBe("1.5\n1.5,2.5,3\n9.99\n3\n3.14\n42\n10,20\n"));
});
