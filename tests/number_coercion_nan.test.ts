import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Casos de borda de Number(): JS spec diz string vazia/whitespace -> 0,
// strings nao-numericas -> NaN.
print(`${Number("")}`);          // 0   (JS spec: string vazia coage para 0)
print(`${Number("   ")}`);       // 0   (JS spec: whitespace coage para 0)
print(`${Number("abc")}`);       // NaN (nao-numerico)
print(`${Number("12abc")}`);     // NaN (lixo no fim)
print(`${Number("  42  ")}`);    // 42  (trim ok)
print(`${Number("3.14")}`);      // 3.14
print(`${Number("-0")}`);        // 0
print(`${Number("1e3")}`);       // 1000

// isNaN sobre o resultado
const a: f64 = Number("");
const b: f64 = Number("xyz");
const c: f64 = Number("7");
print(isNaN(a) ? "yes" : "no"); // no  (Number("") === 0)
print(isNaN(b) ? "yes" : "no"); // yes
print(isNaN(c) ? "yes" : "no"); // no

describe("number_coercion_nan", () => {
  test("Number() segue JS spec (vazio -> 0, lixo -> NaN)", () =>
    expect(__rtsCapturedOutput).toBe(
      "0\n0\nNaN\nNaN\n42\n3.14\n0\n1000\n" +
      "no\nyes\nno\n"
    ));
});
