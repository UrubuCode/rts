import { describe, test, expect } from "rts:test";
import { io, i32 } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

let x: i32 = 5;

if (x > 3) {
    print("big");
} else {
    print("small");
}

if (x === 5) {
    print("five");
}

describe("fixture:if_else", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("big\nfive\n");
  });
});
