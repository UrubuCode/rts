import { describe, test, expect } from "rts:test";
import { io, num } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// saturating_*: clamp em i64::MIN/MAX.

// 9_000_000_000_000_000_000 + 9_000_000_000_000_000_000 saturaria.
const a = num.saturating_add(9000000000000000000, 9000000000000000000);
print(`${a}`);

const b = num.saturating_sub(-9000000000000000000, 9000000000000000000);
print(`${b}`);

const c = num.saturating_mul(3, 7);
print(`${c}`);

describe("fixture:num_saturating", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("9223372036854775807\n-9223372036854775808\n21\n");
  });
});
