import { describe, test, expect } from "rts:test";
import { io, i32 } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

let n: i32 = 42;
print("n=" + n);

let a: i32 = 10;
let b: i32 = 20;
print("a+b=" + (a + b));

print("hello" + " " + "world");

describe("fixture:string_concat", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("n=42\na+b=30\nhello world\n");
  });
});
