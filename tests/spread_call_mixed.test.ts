import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Mistura args normais com spread literal.

function sum4(a: number, b: number, c: number, d: number): number {
    return a + b + c + d;
}

print(`${sum4(10, ...[1, 2], 100)}`); // 113

// Múltiplos spreads.
print(`${sum4(...[1, 2], ...[3, 4])}`); // 10

describe("fixture:spread_call_mixed", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("113\n10\n");
  });
});
