import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Rest param: empacota args num array.

function sum(...nums: number[]): number {
    let total = 0;
    for (const n of nums) {
        total = total + n;
    }
    return total;
}

print(`${sum(1, 2, 3, 4)}`); // 10

describe("fixture:rest_param_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("10\n");
  });
});
