import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

print("hello world");

describe("fixture:print", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("hello world\n");
  });
});
