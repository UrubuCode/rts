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

// (#1281) curry de 3 niveis — args inteiros agora corretos. A arrow liftada
// eh i64-ABI (le params via fcvt_from_sint, espera inteiro); lower_curry_call
// empaca os args como inteiro p/ casar com as capturas (REIFY). NB: capturas
// f64 FRACIONARIAS ainda truncam (limitacao i64-ABI conhecida).
function add3(a: number) { return (b: number) => (c: number) => a + b + c; }
print(add3(1)(2)(3) + "");       // 6
function add4(a: number) {
  return (b: number) => (c: number) => (d: number) => a + b + c + d;
}
print(add4(1)(2)(3)(4) + "");    // 10

describe("call-of-call / curry (#41, #1281)", () => {
  test("f(x)(y) e curry 2-nivel", () =>
    expect(out).toBe("15\n7\n42\n6\n10\n"));
});
