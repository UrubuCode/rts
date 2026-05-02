import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 1. Caso classico
print("hello world");

// 2. String vazia — preserva o "\n" sem comer caractere
print("");

// 3. Escape sequences — \n, \t, \\, \"
print("a\tb\tc");
print("first\nsecond");
print("back\\slash");
print("quote: \"x\"");

// 4. Unicode multi-byte (UTF-8) preservado byte-a-byte
print("ação café");

// 5. Concat de tipos no print(string) — coercao via template
const n: i32 = -42;
const f: f64 = 3.5;
const b: boolean = false;
print(`n=${n} f=${f} b=${b}`);

// 6. Chamadas repetidas preservam ordem e separador
for (let i = 0; i < 3; i = i + 1) {
  print("loop:" + i);
}

// 7. String muito longa nao trunca
let big: string = "";
for (let i = 0; i < 10; i = i + 1) {
  big = big + "0123456789";
}
print(`len=${big.length} first=${big.charAt(0)} last=${big.charAt(99)}`);

// 8. Primeira posicao apos string vazia (regressao: marker entre prints
//    nao pode ser comido por concatenacao subsequente)
print("after-empty");

describe("print — cobertura ampla", () => {
  test("escapes, unicode, concat, loop, long", () => {
    expect(__rtsCapturedOutput).toBe(
      "hello world\n" +
      "\n" +
      "a\tb\tc\n" +
      "first\nsecond\n" +
      "back\\slash\n" +
      "quote: \"x\"\n" +
      "ação café\n" +
      "n=-42 f=3.5 b=false\n" +
      "loop:0\nloop:1\nloop:2\n" +
      "len=100 first=0 last=9\n" +
      "after-empty\n"
    );
  });
});
