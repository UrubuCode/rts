import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Object destructuring com alias: { x: a }.

const obj = { width: 100, height: 50 };
const { width: w, height: h } = obj;

print(`${w}`); // 100
print(`${h}`); // 50

describe("fixture:destruct_alias", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("100\n50\n");
  });
});
