import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// for-in respeita break/continue.

const obj = { a: 1, b: 2, c: 3, d: 4 };

for (const k in obj) {
    if (k == "c") { break; }
    print(k);
}

print("---");

for (const k in obj) {
    if (k == "b") { continue; }
    print(k);
}

describe("fixture:for_in_break", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("a\nb\n---\na\nc\nd\n");
  });
});
