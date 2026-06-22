import { describe, test, expect } from "rts:test";
import { io, math } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Default que é expressão composta + chamada.

function compute(x: number = math.abs_i64(-5)): number {
    return x * 2;
}

print(`${compute()}`);      // 5 * 2 = 10

print(`${compute(7)}`);     // 14

describe("fixture:default_param_expr", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("10\n14\n");
  });
});
