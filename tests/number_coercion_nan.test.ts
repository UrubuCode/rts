import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Casos de borda de Number() onde o parse falha -> NaN.
print(`${Number("")}`);          // NaN (string vazia)
print(`${Number("   ")}`);       // NaN (so whitespace)
print(`${Number("abc")}`);       // NaN (nao-numerico)
print(`${Number("12abc")}`);     // NaN (lixo no fim)
print(`${Number("  42  ")}`);    // 42 (trim ok)
print(`${Number("3.14")}`);      // 3.14
print(`${Number("-0")}`);        // 0
print(`${Number("1e3")}`);       // 1000

// isNaN sobre o resultado
const a: f64 = Number("");
const b: f64 = Number("xyz");
const c: f64 = Number("7");
print(isNaN(a) ? "yes" : "no"); // yes
print(isNaN(b) ? "yes" : "no"); // yes
print(isNaN(c) ? "yes" : "no"); // no

describe("number_coercion_nan", () => {
  test("Number() retorna NaN para parse falha (incl. string vazia)", () =>
    expect(__rtsCapturedOutput).toBe(
      "NaN\nNaN\nNaN\nNaN\n42\n3.14\n0\n1000\n" +
      "yes\nyes\nno\n"
    ));
});
