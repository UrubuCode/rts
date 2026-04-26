import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Deep self-recursion — without TCO this would overflow the stack.
function loopTco(n: i32): i32 {
  if (n <= 0) {
    return 0;
  }
  return loopTco(n - 1);
}

// Tail accumulator.
function sumTail(n: i32, acc: i32): i32 {
  if (n <= 0) {
    return acc;
  }
  return sumTail(n - 1, acc + n);
}

// Mutual tail recursion.
function isEven(n: i32): i32 {
  if (n <= 0) {
    return 1;
  }
  return isOdd(n - 1);
}

function isOdd(n: i32): i32 {
  if (n <= 0) {
    return 0;
  }
  return isEven(n - 1);
}

print(`${loopTco(500000)}`);
print(`${sumTail(100, 0)}`);
print(`${isEven(10000)}`);
print(`${isOdd(10001)}`);

describe("fixture:tail_call", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("0\n5050\n1\n1\n");
  });
});
