import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

function add(a: i64, b: i64): i64 { return a + b; }
function multiply(a: i64, b: i64): i64 { return a * b; }
function noop(): i64 { return 100; }

// 1. fn.call(thisArg, ...args)
print("call=" + add.call(0, 3, 4));

// 2. fn.apply(thisArg, argsArray)
const arr = [10, 20];
print("apply=" + add.apply(0, arr));

// 3. fn.toString() — fn estatica retorna [native code]
const s = noop.toString();
print(s);

// 4. new Function — compila body em runtime via eval
const sq = new Function("x", "return x * x;");
print("new=" + sq.call(0, 9));

// 5. fn.bind — partial application
const dbl = multiply.bind(0, 2);
print("bind=" + dbl.call(0, 7));

// 6. new Function variadic — zero, multi e excess params
const f0 = new Function("return 42;");
print("var0=" + f0.call(0));

const f3 = new Function("a", "b", "c", "return a + b + c;");
print("var3=" + f3.call(0, 1, 2, 3));

// 7. body com loop e condicional
const fsum = new Function("n", "let s: i64 = 0; for (let i: i64 = 1; i <= n; i = i + 1) s = s + i; return s;");
print("sum10=" + fsum.call(0, 10));

// 8. .name e .length getters
const adder = new Function("a", "b", "return a + b;");
print("name=" + adder.name);
print("length=" + adder.length);

// 9. encadeamento + fns coexistindo
const inc = new Function("x", "return x + 1;");
const mul2 = new Function("x", "return x * 2;");
const sub3 = new Function("x", "return x - 3;");
print("chain=" + sub3.call(0, mul2.call(0, inc.call(0, 10))));

// 10. new Function dentro de user fn
function makeAdd100(n: i64): i64 {
    const f = new Function("x", "return x + 100;");
    return f.call(0, n);
}
print("nest=" + makeAdd100(5));

// 11. bind reduz .length corretamente
const bndlen = new Function("a", "b", "c", "return a + b + c;");
const bndlen_p = bndlen.bind(0, 100);
print("bnd_len=" + bndlen_p.length);
print("bnd_call=" + bndlen_p.call(0, 5, 10));

// 12. aliasing — `const b = a` propaga local_class_ty
const orig = new Function("x", "return x + 1;");
const alias = orig;
print("alias=" + alias.call(0, 41));

describe("Function global (#359)", () => {
  test("call/apply/toString/new/bind + variadic + getters", () => {
    expect(__rtsCapturedOutput).toBe(
      "call=7\napply=30\nfunction noop() { [native code] }\nnew=81\nbind=14\nvar0=42\nvar3=6\nsum10=55\nname=anonymous\nlength=2\nchain=19\nnest=105\nbnd_len=2\nbnd_call=115\nalias=42\n"
    );
  });
});
