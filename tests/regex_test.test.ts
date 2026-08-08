import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Regex literal + test(). regex.test(re, s) era a forma livre; em JS o
// test e' metodo do proprio RegExp. A flag i continua sendo o que faz
// "USER@MAIL.COM" casar um padrao escrito em minusculas.

const re = /^[a-z]+@[a-z]+\.[a-z]+$/i;
const ok = re.test("USER@MAIL.COM") ? 1 : 0;
const bad = re.test("not-an-email") ? 1 : 0;
print(`${ok}`); // 1
print(`${bad}`); // 0

describe("fixture:regex_test", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("1\n0\n");
  });
});
