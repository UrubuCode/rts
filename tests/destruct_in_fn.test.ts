import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Destructuring dentro de fn user.

function process(): number {
    const arr = [7, 13, 100];
    const [first, second] = arr;
    return first + second;
}

print(`${process()}`); // 20

describe("fixture:destruct_in_fn", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("20\n");
  });
});
