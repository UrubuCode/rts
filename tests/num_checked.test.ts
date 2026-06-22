import { describe, test, expect } from "rts:test";
import { io, num } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// num.checked_*: aritmetica que sinaliza overflow via i64::MIN.

const a = num.checked_add(100, 200);
print(`${a}`);

const b = num.checked_div(100, 0);
print(`${b}`);

const c = num.checked_sub(50, 30);
print(`${c}`);

const d = num.checked_mul(7, 6);
print(`${d}`);

describe("fixture:num_checked", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("300\n-9223372036854775808\n20\n42\n");
  });
});
