import { describe, test, expect } from "rts:test";
import { io, i32, string } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 1. Caso classico — number coerce + string + string
const n: i32 = 42;
print("n=" + n);

const a: i32 = 10;
const b: i32 = 20;
print("a+b=" + (a + b));

print("hello" + " " + "world");

// 2. String vazia — preserva, nao some
print("[" + "" + "]");
print("" + "x" + "");
print("blen=" + string.byte_len("" + "" + ""));   // blen=0

// 3. Cadeia longa — varias concatenacoes consecutivas
const s1 = "a" + "b" + "c" + "d" + "e" + "f" + "g";
print(`len=${string.byte_len(s1)} val=${s1}`);

// 4. Concat dentro de loop acumula corretamente
let acc: string = "";
for (let i = 0; i < 5; i = i + 1) {
  acc = acc + i + ",";
}
print(acc);                          // "0,1,2,3,4,"
print(`len=${string.byte_len(acc)}`);// 10

// 5. Mistura tipos coerce para string (number + string segue ordem AST)
const r1 = 1 + 2 + "x";              // "3x"  (1+2=3 number, depois "3"+"x")
const r2 = "x" + 1 + 2;              // "x12" (concat preserva)
print(r1);
print(r2);

// 6. Concat com retorno de fn
function tag(): string { return "T"; }
print(tag() + "/" + tag() + "/" + tag());   // "T/T/T"

// 7. Compound assign string
let buf: string = "";
buf += "p";
buf += "i";
buf += "n";
buf += "g";
print(buf);                          // "ping"

// 8. Concat preserva escapes em literal
print("a\tb" + "\nC" + "\\D");

describe("string concat — empty, chains, loop, mix tipos", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "n=42\na+b=30\nhello world\n" +
      "[]\nx\nblen=0\n" +
      "len=7 val=abcdefg\n" +
      "0,1,2,3,4,\nlen=10\n" +
      "3x\nx12\n" +
      "T/T/T\n" +
      "ping\n" +
      "a\tb\nC\\D\n"
    );
  });
});
