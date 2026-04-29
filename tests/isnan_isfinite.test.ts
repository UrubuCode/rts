import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#298) isNaN/isFinite globais. Antes davam "undeclared user function".

// 1. isNaN basico
print(`${isNaN(NaN)}`);          // true
print(`${isNaN(5)}`);            // false
print(`${isNaN(0)}`);            // false
print(`${isNaN(-1.5)}`);         // false
print(`${isNaN(Infinity)}`);     // false
print(`${isNaN(-Infinity)}`);    // false

// 2. isFinite basico
print(`${isFinite(5)}`);         // true
print(`${isFinite(-1.5)}`);      // true
print(`${isFinite(0)}`);         // true
print(`${isFinite(NaN)}`);       // false
print(`${isFinite(Infinity)}`);  // false
print(`${isFinite(-Infinity)}`); // false

// 3. Em conditional — guard antes de usar valor
function safeReciprocal(x: f64): f64 {
  if (isNaN(x)) return 0.0;
  if (!isFinite(x)) return 0.0;
  return 1.0 / x;
}
const a: f64 = 4.0;
print(`${safeReciprocal(a)}`);    // 0.25

const b: f64 = NaN;
print(`${safeReciprocal(b)}`);    // 0

// 4. Aritmetica que produz NaN/Infinity, depois isNaN/isFinite
const c: f64 = 0.0;
const d: f64 = 0.0;
const result: f64 = c / d;        // NaN
print(`${isNaN(result)}`);        // true
print(`${isFinite(result)}`);     // false

// 5. Aritmetica em sequencia + isFinite filter
let sum: f64 = 0.0;
const v1: f64 = 1.0;
const v2: f64 = NaN;
const v3: f64 = 3.0;
if (isFinite(v1)) sum = sum + v1;
if (isFinite(v2)) sum = sum + v2;
if (isFinite(v3)) sum = sum + v3;
print(`${sum}`);                  // 4 (1+3)

describe("isnan_isfinite", () => {
  test("globais NaN/Finite checks", () =>
    expect(__rtsCapturedOutput).toBe(
      "true\nfalse\nfalse\nfalse\nfalse\nfalse\n" +     // 1
      "true\ntrue\ntrue\nfalse\nfalse\nfalse\n" +       // 2
      "0.25\n0\n" +                                     // 3
      "true\nfalse\n" +                                 // 4
      "4\n"                                             // 5
    ));
});
