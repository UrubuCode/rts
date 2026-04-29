import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#299) String + Number e' concat na ordem do AST. Bug previo: peephole
// de Add invertia lhs/rhs quando literal numerico estava a esquerda,
// produzindo \`3 + "5" = "53"\` em vez de \`"35"\`.

// 1. Casos basicos do issue
print(`${"5" + 3}`);     // "53"
print(`${3 + "5"}`);     // "35"
print(`${"abc" + 5}`);   // "abc5"
print(`${5 + "abc"}`);   // "5abc"

// 2. String + string nao afetado
print(`${"a" + "b"}`);   // "ab"
print(`${"" + "x"}`);    // "x"

// 3. Number + number ainda funciona normalmente
print(`${1 + 2}`);       // 3
print(`${10 + 5}`);      // 15

// 4. Encadeamento — JS avalia left-to-right
print(`${1 + 2 + "3"}`);    // "33" (1+2=3 numero, depois 3+"3"="33")
print(`${"1" + 2 + 3}`);    // "123" (concat preserva: "12" depois "123")
print(`${1 + "2" + 3}`);    // "123" (1+"2"="12" depois +"3"="123")

// 5. Em fn user
function concat(a: number, b: string): string {
  return a + b;
}
print(concat(7, "x"));      // "7x"
print(`${42 + "test"}`);    // "42test"

// 6. Em template — multiplas expressoes
const n: number = 5;
const s: string = "items";
print(`have ${n} ${s}, total: ${n + " " + s}`);  // "have 5 items, total: 5 items"

// 7. Compound assign
let acc: string = "";
acc += 1;
acc += 2;
acc += "x";
print(acc);   // "12x"

// 8. Em conditional (ternario com string concat)
const flag: boolean = true;
const r: string = flag ? 1 + "x" : "none";
print(r);   // "1x"

// 9. Negativos preservam ordem
print(`${-3 + "z"}`);   // "-3z"
print(`${"z" + -3}`);   // "z-3"

describe("string_number_concat", () => {
  test("ordem do AST preservada em concat", () =>
    expect(__rtsCapturedOutput).toBe(
      "53\n35\nabc5\n5abc\n" +              // 1
      "ab\nx\n" +                           // 2
      "3\n15\n" +                           // 3
      "33\n123\n123\n" +                    // 4
      "7x\n42test\n" +                      // 5
      "have 5 items, total: 5 items\n" +    // 6
      "12x\n" +                             // 7
      "1x\n" +                              // 8
      "-3z\nz-3\n"                          // 9
    ));
});
