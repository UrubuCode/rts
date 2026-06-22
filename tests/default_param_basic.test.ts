import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Default parameter: chamada sem o arg usa o default.

function add(a: number, b: number = 10): number {
    return a + b;
}

print(`${add(5)}`); // 15 (5 + default 10)

print(`${add(5, 100)}`); // 105

describe("fixture:default_param_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("15\n105\n");
  });
});
