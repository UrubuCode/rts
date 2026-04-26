import { describe, test, expect } from "rts:test";
import { io, gc, num } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// num.bit ops: count_*, leading/trailing_zeros, rotate, reverse.

// 0xFF = 8 ones
const a = num.count_ones(255);
const h1 = gc.string_from_i64(a); print(h1); gc.string_free(h1);

// 1 << 0 = 1: 63 leading zeros (i64).
const b = num.leading_zeros(1);
const h2 = gc.string_from_i64(b); print(h2); gc.string_free(h2);

// 8: 3 trailing zeros.
const c = num.trailing_zeros(8);
const h3 = gc.string_from_i64(c); print(h3); gc.string_free(h3);

// rotate_left(1, 4) = 16
const d = num.rotate_left(1, 4);
const h4 = gc.string_from_i64(d); print(h4); gc.string_free(h4);

// swap_bytes(0x12) = 0x1200000000000000 (signed: negativo grande)
const e = num.swap_bytes(0x12);
const h5 = gc.string_from_i64(e); print(h5); gc.string_free(h5);

describe("fixture:num_bits", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("8\n63\n3\n16\n1297036692682702848\n");
  });
});
