import { describe, test, expect } from "rts:test";
import { io, string } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 1. Igualdade basica entre literais (caso historico)
const a = "hello";
const b = "hello";
const c = "world";
print(`a == b: ${a == b}`);
print(`a == c: ${a == c}`);
print(`a != c: ${a != c}`);

// 2. char_at retorna string handle — comparacao por conteudo
const s = "abc";
const ch = string.char_at(s, 1);
print(`ch == "b": ${ch == "b"}`);
print(`ch == "x": ${ch == "x"}`);

// 3. Strings vazias
const e1 = "";
const e2 = "";
print(`empty == empty: ${e1 == e2}`);
print(`empty == nonempty: ${e1 == "x"}`);

// 4. Mesmo conteudo via concat (handles diferentes, conteudo igual)
const concat1 = "foo" + "bar";
const concat2 = "foob" + "ar";
print(`concat == lit: ${concat1 == "foobar"}`);
print(`concat1 == concat2: ${concat1 == concat2}`);

// 5. Case sensitive
print(`abc == ABC: ${"abc" == "ABC"}`);

// 6. Substrings (prefix, suffix) — nao sao iguais
print(`abc == ab: ${"abc" == "ab"}`);
print(`ab == abc: ${"ab" == "abc"}`);

// 7. Comparacao via === (strict) — sem coercion
print(`abc === abc: ${"abc" === "abc"}`);
print(`1 === "1": ${1 === "1"}`);

// 8. Em condicional (regressao: comparacao em if-stmt)
function classify(v: string): string {
  if (v == "yes") return "Y";
  if (v == "no")  return "N";
  return "?";
}
print(classify("yes"));
print(classify("no"));
print(classify("maybe"));

// 9. Em loop com array — comparacao multipla. Nota: precisa
// declarar `let p: string` separado pra herdar o tipo no bind.
const pets = ["cat", "dog", "cat", "fish"];
let cats: i32 = 0;
let p: string = "";
for (p of pets) {
  if (p == "cat") cats = cats + 1;
}
print(`cats=${cats}`);

// 10. Strings longas — mesma boundary length
const long_a = "0123456789012345678901234567890123456789";
const long_b = "0123456789012345678901234567890123456789";
const long_c = "0123456789012345678901234567890123456788"; // ultimo char diferente
print(`long_a == long_b: ${long_a == long_b}`);
print(`long_a == long_c: ${long_a == long_c}`);

describe("string == — empty, concat, case, condicional, loop", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "a == b: true\na == c: false\na != c: true\n" +
      "ch == \"b\": true\nch == \"x\": false\n" +
      "empty == empty: true\nempty == nonempty: false\n" +
      "concat == lit: true\nconcat1 == concat2: true\n" +
      "abc == ABC: false\n" +
      "abc == ab: false\nab == abc: false\n" +
      "abc === abc: true\n1 === \"1\": false\n" +
      "Y\nN\n?\n" +
      "cats=2\n" +
      "long_a == long_b: true\nlong_a == long_c: false\n"
    );
  });
});
