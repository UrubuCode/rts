import { describe, test, expect } from "rts:test";
import { io, num } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// num.bit ops: count_*, leading/trailing_zeros, rotate, reverse.

// 0xFF = 8 ones
const a = num.count_ones(255);
print(`${a}`);

// 1 << 0 = 1: 63 leading zeros (i64).
const b = num.leading_zeros(1);
print(`${b}`);

// 8: 3 trailing zeros.
const c = num.trailing_zeros(8);
print(`${c}`);

// rotate_left(1, 4) = 16
const d = num.rotate_left(1, 4);
print(`${d}`);

// swap_bytes(0x12) = 0x1200000000000000 (signed: negativo grande)
const e = num.swap_bytes(0x12);
print(`${e}`);

describe("fixture:num_bits", () => {
  test("matches expected stdout", () => {
    // swap_bytes > 2^53 (0x1200000000000000): sem repr Int64, vira f64 e imprime
    // como JS/Node (1297036692682702800). Era exato no motor velho (deletado).
    expect(__rtsCapturedOutput).toBe("8\n63\n3\n16\n1297036692682702800\n");
  });
});
