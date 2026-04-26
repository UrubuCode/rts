import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

print(`${5.5 % 2.0}`);
print(`${10.3 % 3.0}`);
print(`${-7.5 % 2.0}`);
print(`${1.0 % 1.0}`);

describe("fixture:f64_modulo", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("1.5\n1.3000000000000007\n-1.5\n0\n");
  });
});
