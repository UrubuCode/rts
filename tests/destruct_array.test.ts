import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Array destructuring básico.

const [a, b, c] = [10, 20, 30];

print(`${a}`); // 10
print(`${b}`); // 20
print(`${c}`); // 30

describe("fixture:destruct_array", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("10\n20\n30\n");
  });
});
