import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Spread de array literal em chamada de fn user.

function add3(a: number, b: number, c: number): number {
    return a + b + c;
}

print(`${add3(...[1, 2, 3])}`); // 6

describe("fixture:spread_call_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("6\n");
  });
});
