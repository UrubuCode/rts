import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Spread no callsite + rest no callee — interação correta.

function sumAll(...nums: number[]): number {
    let total = 0;
    for (const n of nums) {
        total = total + n;
    }
    return total;
}

const h = gc.string_from_i64(sumAll(...[10, 20, 30, 40]));
print(h); gc.string_free(h); // 100

describe("fixture:spread_with_rest", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("100\n");
  });
});
