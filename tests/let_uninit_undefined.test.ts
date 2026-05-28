import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `let u;` / `let u: any;` sem init recebia 0 em
// vez de undefined. Consequencia: `typeof u` -> "number", `u === undefined`
// false, template "0". Fix: var sem init e sem tipo concreto inicializa com
// sentinel i64::MIN+2 (undefined) e eh marcada ambigua (typeof via runtime).
// Vars tipadas (`let n: number`) mantem zero_for_ty.

let out = "";
function print(v: string): void { out += v + "\n"; }

let u: any;
print(typeof u);                 // undefined
print((u === undefined) + "");   // true
print((u ?? "fb"));              // fb
print("" + u);                   // undefined

let u2;
print(typeof u2);                // undefined

// var tipada continua usavel apos atribuicao
let n: number;
n = 5;
print(n + "");                   // 5
let s: string;
s = "hi";
print(s);                        // hi

// atribuir a var any depois funciona
let v: any;
v = 42;
print(typeof v);                 // number
print(v + "");                   // 42

describe("let sem init -> undefined", () => {
  test("var sem init e' undefined, tipada e' usavel", () =>
    expect(out).toBe("undefined\ntrue\nfb\nundefined\nundefined\n5\nhi\nnumber\n42\n"));
});
