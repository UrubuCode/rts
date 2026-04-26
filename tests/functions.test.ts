import { describe, test, expect } from "rts:test";
import { io, i32 } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

function add(a: i32, b: i32): i32 {
    return a + b;
}

function factorial(n: i32): i32 {
    if (n <= 1) {
        return 1;
    }
    return n * factorial(n - 1);
}

let result: i32 = add(3, 4);
print("add:" + result);

let fact5: i32 = factorial(5);
print("fact5:" + fact5);

describe("fixture:functions", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("add:7\nfact5:120\n");
  });
});
