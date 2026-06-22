import { describe, test, expect } from "rts:test";
import { io, num } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// wrapping_*: aritmetica modular.

// MAX + 1 -> MIN (modular wrap).
const a = num.wrapping_add(9223372036854775807, 1);
print(`${a}`);

// 0 - 1 -> -1
const b = num.wrapping_sub(0, 1);
print(`${b}`);

const c = num.wrapping_neg(42);
print(`${c}`);

const d = num.wrapping_shl(1, 4);
print(`${d}`);

const e = num.wrapping_shr(256, 4);
print(`${e}`);

describe("fixture:num_wrapping", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("-9223372036854775808\n-1\n-42\n16\n16\n");
  });
});
