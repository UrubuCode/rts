import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Generic function operando em array tipado.

function first<T>(arr: T[]): T {
  return arr[0];
}

function last<T>(arr: T[], len: i64): T {
  return arr[len - 1];
}

const xs: i64[] = [10, 20, 30, 40];
const a = first<i64>(xs);
const b = last<i64>(xs, 4);

print(`${a}`);
print(`${b}`);

describe("fixture:generic_first_array", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("10\n40\n");
  });
});
