import { describe, test, expect } from "rts:test";

// (issue-pai invoke/param_kinds) HOF: user fn NOMEADA passada como valor e
// chamada. Antes: `apply(inc,10)` -> lixo (func_addr cru sem param_kinds;
// invoke_n tratava (f64->f64) como (i64->i64)). Fix em 3 partes:
// (a) reificar arg user-fn-nomeada como handle com param_kinds/return_kind;
// (b) INVOKE_AUTO_AS_F64 normaliza retorno p/ f64 (handle f64-ret OU fn_ptr
//     raw i64-ret); (c) normalizar args bits-f64 -> i64 p/ fn_ptr raw i64-param.

let out = "";
function print(v: string): void { out += v + "\n"; }

function inc(n: number): number { return n + 1; }
function dbl(n: number): number { return n * 2; }
function apply(f: (n: number) => number, n: number): number { return f(n); }

print((apply(inc, 10)) + "");        // 11
print((apply(dbl, 21)) + "");        // 42
print((apply(dbl, 2.5)) + "");       // 5 (float nao truncado)

// function expression inline passada a HOF
print((apply(function(x: number): number { return x + 10; }, 5)) + "");  // 15

// applyTwice (chamada aninhada)
function applyTwice(f: (n: number) => number, n: number): number { return f(f(n)); }
print((applyTwice(inc, 10)) + "");   // 12

// first-class i32 (guard: nao deve regredir)
function double(x: i32): i32 { return x * 2; }
function applyI(fn: i64, x: i32): i32 { return fn(x); }
print((applyI(double, 5)) + "");     // 10

describe("HOF fn-param (issue-pai invoke)", () => {
  test("user fn passada como valor e chamada", () =>
    expect(out).toBe("11\n42\n5\n15\n12\n10\n"));
});
