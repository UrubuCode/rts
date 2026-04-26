import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

print(`${2 ** 10}`);
print(`${2.0 ** 0.5}`);
print(`${3 ** 3}`);
print(`${10 ** -2}`);

describe("fixture:exponentiation", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("1024\n1.4142135623730951\n27\n0.01\n");
  });
});
