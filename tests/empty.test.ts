import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// empty program — should exit 0 with no output

describe("fixture:empty", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("");
  });
});
