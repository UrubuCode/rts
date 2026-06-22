import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Generic function: identity<T>.

function identity<T>(x: T): T {
  return x;
}

const a = identity<i64>(42);
print(`${a}`);

const b = identity<i64>(-7);
print(`${b}`);

describe("fixture:generic_identity", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("42\n-7\n");
  });
});
