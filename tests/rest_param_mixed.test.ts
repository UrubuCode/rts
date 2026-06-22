import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Mistura param normal + rest.

function joinFrom(start: number, ...extra: number[]): number {
    let total = start;
    for (const n of extra) {
        total = total + n;
    }
    return total;
}

print(`${joinFrom(100, 1, 2, 3)}`); // 106
print(`${joinFrom(50)}`); // 50

describe("fixture:rest_param_mixed", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("106\n50\n");
  });
});
