import { describe, test, expect } from "rts:test";
import { io, i32, str } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

let x: i32 = 10;
let y: i32 = 3;
let sum: i32 = x + y;
let diff: i32 = x - y;
let prod: i32 = x * y;
let quot: i32 = x / y;
let rem: i32 = x % y;

print("sum:" + sum);
print("diff:" + diff);
print("prod:" + prod);
print("quot:" + quot);
print("rem:" + rem);

describe("fixture:arithmetic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("sum:13\ndiff:7\nprod:30\nquot:3\nrem:1\n");
  });
});
