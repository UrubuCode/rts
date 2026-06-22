import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Generic constraint <T extends X>: type-erased em runtime.

function add<T extends i64>(a: T, b: T): T {
  return a + b;
}

function max<T extends i64>(a: T, b: T): T {
  if (a > b) return a;
  return b;
}

const r1 = add<i64>(7, 8);
print(`${r1}`);

const r2 = max<i64>(15, 23);
print(`${r2}`);

describe("fixture:generic_constraint", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("15\n23\n");
  });
});
