import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Default com literal string.

function greet(name: string = "world"): void {
    print(name);
}

greet();
greet("Alice");

describe("fixture:default_param_string", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("world\nAlice\n");
  });
});
