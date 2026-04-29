import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Globais Number(), String(), Boolean() — conversoes JS explicitas.

// 1. Number()
print(`${Number("42")}`);          // 42
print(`${Number("3.14")}`);        // 3.14
print(`${Number("-7")}`);          // -7
print(`${Number("abc")}`);         // NaN
print(`${Number("")}`);            // NaN (string vazia parse falha)
print(`${Number(true)}`);          // 1
print(`${Number(false)}`);         // 0
print(`${Number(0)}`);             // 0

// 2. String()
print(String(0));                  // "0"
print(String(123));                // "123"
print(String(-1));                 // "-1"
print(String(true));               // "true"
print(String(false));              // "false"
print(String("already"));          // "already"

// 3. Boolean() — truthiness JS
print(`${Boolean(0)}`);            // false
print(`${Boolean(1)}`);            // true
print(`${Boolean(-1)}`);           // true (qualquer nao-zero)
print(`${Boolean(true)}`);         // true
print(`${Boolean(false)}`);        // false

// 4. Em fn user — coerce input antes de processar
// (\`: f64\` explicito porque \`: number\` em RTS atualmente vira I32,
// perdendo NaN — issue #323)
function safeAge(s: string): f64 {
  const n: f64 = Number(s);
  if (isNaN(n)) return -1.0;
  return n;
}
print(`${safeAge("25")}`);         // 25
print(`${safeAge("xx")}`);         // -1

// 5. String concat com Number/String coerce
const x: number = 42;
print("x = " + String(x));         // "x = 42"
print(String(x) + " years");       // "42 years"

// 6. Boolean em conditional
const v: number = 5;
const flag = Boolean(v);
print(flag ? "truthy" : "falsy"); // "truthy"

const v2: number = 0;
const flag2 = Boolean(v2);
print(flag2 ? "truthy" : "falsy"); // "falsy"

// 7. Number em aritmetica direta
const total: number = Number("10") + Number("20");
print(`${total}`);  // 30

describe("js_conversions", () => {
  test("Number/String/Boolean globais", () =>
    expect(__rtsCapturedOutput).toBe(
      "42\n3.14\n-7\nNaN\nNaN\n1\n0\n0\n" +              // 1
      "0\n123\n-1\ntrue\nfalse\nalready\n" +              // 2
      "false\ntrue\ntrue\ntrue\nfalse\n" +                // 3
      "25\n-1\n" +                                        // 4
      "x = 42\n42 years\n" +                              // 5
      "truthy\nfalsy\n" +                                 // 6
      "30\n"                                              // 7
    ));
});
