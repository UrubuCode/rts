import { describe, test, expect } from "rts:test";
import { gc, buffer } from "rts";

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
const eh = gc.string_from_static(eq ? "eq" : "neq");
print(eh); gc.string_free(eh);

buffer.write_u8(b, 4, 9);
const neq = buffer.equals(a, b);
const nh = gc.string_from_static(neq ? "still-eq" : "diff");
print(nh); gc.string_free(nh);

// indexOf 'C' (67) — pos 2
const i1 = buffer.index_of(a, 67, 0);
const ih = gc.string_from_i64(i1);
print(ih); gc.string_free(ih);

// indexOf de byte ausente
const i2 = buffer.index_of(a, 200, 0);
const i2h = gc.string_from_i64(i2);
print(i2h); gc.string_free(i2h);

// indexOf com from > size
const i3 = buffer.index_of(a, 65, 100);
const i3h = gc.string_from_i64(i3);
print(i3h); gc.string_free(i3h);

buffer.free(a); buffer.free(b);

describe("fixture:buffer_equals_indexof", () => {
  test("equals + index_of edge cases", () => {
    expect(__rtsCapturedOutput).toBe("eq\ndiff\n2\n-1\n-1\n");
  });
});
