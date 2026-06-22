import { describe, test, expect } from "rts:test";
import { buffer } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Adicionados em #289 follow-up: buffer.equals + buffer.index_of.

const a = buffer.alloc(5);
const b = buffer.alloc(5);
for (let i: i64 = 0; i < 5; i = i + 1) {
  buffer.write_u8(a, i, i + 65);
  buffer.write_u8(b, i, i + 65);
}

const eq = buffer.equals(a, b);
print(eq ? "eq" : "neq");

buffer.write_u8(b, 4, 9);
const neq = buffer.equals(a, b);
print(neq ? "still-eq" : "diff");

// indexOf 'C' (67) — pos 2
const i1 = buffer.index_of(a, 67, 0);
print(`${i1}`);

// indexOf de byte ausente
const i2 = buffer.index_of(a, 200, 0);
print(`${i2}`);

// indexOf com from > size
const i3 = buffer.index_of(a, 65, 100);
print(`${i3}`);

buffer.free(a); buffer.free(b);

describe("fixture:buffer_equals_indexof", () => {
  test("equals + index_of edge cases", () => {
    expect(__rtsCapturedOutput).toBe("eq\ndiff\n2\n-1\n-1\n");
  });
});
