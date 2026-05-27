import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): literal numerico inteiro grande (`1e21`,
// `1e20`) que NAO cabe em i64 saturava em i64::MAX (9223372036854775807)
// porque o lowering fazia `v as i64` sem checar o range. JS: number eh
// sempre f64. Fix: literal inteiro so' vira iconst se cabe no range i64;
// senao mantem f64const.

let out = "";
function print(v: string): void { out += v + "\n"; }

const a = 1e20;
print("" + a);            // 100000000000000000000
const b = 1e21;
print("" + b);            // 1e+21
print("" + (b > a));      // true
const big = 9e18;         // ~cabe i64 (i64::MAX ~9.2e18) — limite
print("" + (big > 1e18)); // true

// valores que CABEM em i64 continuam como int exato
const small = 1000000;
print("" + small);        // 1000000

describe("number literal overflow i64", () => {
  test("literal grande nao satura", () =>
    expect(out).toBe("100000000000000000000\n1e+21\ntrue\ntrue\n1000000\n"));
});
