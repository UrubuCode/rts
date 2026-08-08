import { describe, test, expect } from "rts:test";
import { Buffer } from "node:buffer";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Adicionados em #289 follow-up: os mesmos casos de borda, agora sobre o
// `Buffer` do `node:buffer` — `write_u8` vira indexação, `index_of` vira
// `indexOf`, e `free` desaparece porque um Buffer é coletado (não há handle
// explícito a devolver). Os valores esperados são os do Node.

const a = Buffer.alloc(5);
const b = Buffer.alloc(5);
for (let i: i64 = 0; i < 5; i = i + 1) {
  a[i] = i + 65;
  b[i] = i + 65;
}

const eq = a.equals(b);
print(eq ? "eq" : "neq");

b[4] = 9;
const neq = a.equals(b);
print(neq ? "still-eq" : "diff");

// indexOf 'C' (67) — pos 2
const i1 = a.indexOf(67, 0);
print(`${i1}`);

// indexOf de byte ausente
const i2 = a.indexOf(200, 0);
print(`${i2}`);

// indexOf com from > size
const i3 = a.indexOf(65, 100);
print(`${i3}`);

describe("fixture:buffer_equals_indexof", () => {
  test("equals + index_of edge cases", () => {
    expect(__rtsCapturedOutput).toBe("eq\ndiff\n2\n-1\n-1\n");
  });
});
