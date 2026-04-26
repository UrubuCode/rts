import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Mistura args normais com spread literal.

function sum4(a: number, b: number, c: number, d: number): number {
    return a + b + c + d;
}

const h1 = gc.string_from_i64(sum4(10, ...[1, 2], 100));
print(h1); gc.string_free(h1); // 113

// Múltiplos spreads.
const h2 = gc.string_from_i64(sum4(...[1, 2], ...[3, 4]));
print(h2); gc.string_free(h2); // 10

describe("fixture:spread_call_mixed", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("113\n10\n");
  });
});
