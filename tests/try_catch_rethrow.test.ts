import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// catch que faz rethrow — outer catch pega.

try {
    try {
        throw "first";
    } catch (e) {
        print(`inner: ${e}`);
        throw "rethrown";
    }
} catch (e2) {
    print(`outer: ${e2}`);
}

print("end");

describe("fixture:try_catch_rethrow", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("inner: first\nouter: rethrown\nend\n");
  });
});
