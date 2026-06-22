import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Object destructuring básico.

const obj = { x: 5, y: 10 };
const { x, y } = obj;

print(`${x}`); // 5
print(`${y}`); // 10

describe("fixture:destruct_object", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("5\n10\n");
  });
});
