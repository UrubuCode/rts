import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Numeric enum básico, auto-incremento de 0.

enum Status {
    Pending,
    Active,
    Closed,
}

print(`${Status.Pending}`); // 0
print(`${Status.Active}`); // 1
print(`${Status.Closed}`); // 2

describe("fixture:enum_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("0\n1\n2\n");
  });
});
