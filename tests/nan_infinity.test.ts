import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#298) Globais JS NaN, Infinity, undefined antes davam undefined-var.

// 1. Stringificacao em template
print(`${NaN}`);          // NaN
print(`${Infinity}`);     // Infinity
print(`${-Infinity}`);    // -Infinity
print(`${undefined}`);    // undefined

// 2. typeof
print(`${typeof NaN}`);        // number
print(`${typeof Infinity}`);   // number
print(`${typeof undefined}`);  // undefined
print(`${typeof 5}`);          // number
print(`${typeof "x"}`);        // string

// 3. NaN === NaN sempre false (regra JS)
print(`${NaN === NaN}`);   // false
print(`${NaN !== NaN}`);   // true
print(`${NaN == NaN}`);    // false

// 4. Infinity comparisons
print(`${Infinity > 1e308}`);     // true
print(`${-Infinity < -1e308}`);   // true
print(`${Infinity === Infinity}`); // true

// 5. Aritmetica f64 IEEE — via vars pra evitar peephole de literais.
// Anotacao \`: f64\` explicita pra f64; \`number\` em var decl tem bug
// separado que faz tipo virar int (a investigar).
const one: f64 = 1.0;
const minus_one: f64 = -1.0;
const zero: f64 = 0.0;
print(`${one / zero}`);     // Infinity
print(`${minus_one / zero}`); // -Infinity
print(`${zero / zero}`);    // NaN
print(`${Infinity - Infinity}`);  // NaN
print(`${Infinity + 1}`);          // Infinity

// 6. Em fn user
function safeDiv(a: number, b: number): number {
  return a / b;  // se b=0, retorna +/- Infinity
}
print(`${safeDiv(10, 0)}`);     // Infinity
print(`${safeDiv(-10, 0)}`);    // -Infinity

// 7. Em conditional — NaN === NaN sempre false, branch alt
const x: f64 = NaN;
const tag: string = (x === x) ? "ordered" : "NaN";
print(tag);  // NaN

// 8. NaN no Math
import { math } from "rts";
print(`${math.sqrt(-1)}`);  // NaN

describe("nan_infinity", () => {
  test("globals + IEEE-754 propagation", () =>
    expect(__rtsCapturedOutput).toBe(
      "NaN\nInfinity\n-Infinity\nundefined\n" +     // 1
      "number\nnumber\nundefined\nnumber\nstring\n" + // 2
      "false\ntrue\nfalse\n" +                       // 3
      "true\ntrue\ntrue\n" +                         // 4
      "Infinity\n-Infinity\nNaN\nNaN\nInfinity\n" +  // 5
      "Infinity\n-Infinity\n" +                      // 6
      "NaN\n" +                                      // 7
      "NaN\n"                                        // 8
    ));
});
