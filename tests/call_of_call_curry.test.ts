import { describe, test, expect } from "rts:test";

// Regression (cross-runtime #41): `f(x)(y)` (call-of-call / curry) dava
// "unsupported call expression form" — o callee sendo ele proprio uma CallExpr
// nao era coberto. Fix: fallback p/ lower_indirect_call quando o callee eh
// Call/Paren/Cond/Bin (produz fn_ptr/handle Function).

let out = "";
function print(v: string): void { out += v + "\n"; }

function nested(base: number) {
  return function (extra: number): number { return base + extra; };
}
print(nested(10)(5) + "");       // 15

// curry com arrow
function adder(a: number) {
  return (b: number) => a + b;
}
print(adder(3)(4) + "");         // 7

// (expr)(args) via paren
const f = (x: number) => x * 2;
print((f)(21) + "");             // 42

// NOTA: curry de 3+ niveis (`add3(1)(2)(3)`) ainda tem residuo do elo
// f64/invoke encadeado (3o nivel perde precisao) — follow-up.

describe("call-of-call / curry (#41)", () => {
  test("f(x)(y) e curry 2-nivel", () =>
    expect(out).toBe("15\n7\n42\n"));
});
