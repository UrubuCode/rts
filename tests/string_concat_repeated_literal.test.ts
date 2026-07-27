import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Bug regression: string concat com mesmo literal repetido na mesma
// expressao retornava resultado quebrado (segundo literal sumia).
// Causa: lower_add liberava operandos "fresh" apos concat, mas o
// str_handle_cache retornava o handle ja' liberado no segundo uso.
// Fix: nao liberar handle que esta no str_handle_cache.

// Caso 1: literal repetido com tab (caso original que motivou fix)
const r1 = "x" + "\t" + "y" + "\t" + "z";
print("r1=" + new TextEncoder().encode(r1).length + ":" + r1);

// Caso 2: variavel + literais repetidos (caso do http_chat)
const id: i64 = 1;
const user = "a";
const text = "abc";
const r2 = id + "\t" + user + "\t" + text + "\n";
print("r2=" + new TextEncoder().encode(r2).length);

// Caso 3: literais identicos repetidos (mesmo conteudo)
const r3 = "a" + "1" + "b" + "1" + "c";
print("r3=" + new TextEncoder().encode(r3).length + ":" + r3);

// Caso 4: 2 literais distintos repetidos alternados
const r4 = "x" + "Y" + "x" + "Y" + "z";
print("r4=" + new TextEncoder().encode(r4).length + ":" + r4);

describe("string concat with repeated literals (#chat-tab fix)", () => {
  test("preserva todos os literais", () => {
    expect(__rtsCapturedOutput).toBe(
      "r1=5:x\ty\tz\nr2=8\nr3=5:a1b1c\nr4=5:xYxYz\n"
    );
  });
});
