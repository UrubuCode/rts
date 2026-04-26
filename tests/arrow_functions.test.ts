import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

const double = (x: i32): i32 => x * 2;
const sum = (a: i32, b: i32): i32 => a + b;
const greet = (name: string): string => `hello ${name}`;
const answer = (): i32 => 42;

const triple = (x: i32): i32 => {
  return x * 3;
};

print(`double(5) = ${double(5)}`);
print(`sum(3, 4) = ${sum(3, 4)}`);
print(greet("arrow"));
print(`answer = ${answer()}`);
print(`triple(7) = ${triple(7)}`);

describe("fixture:arrow_functions", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("double(5) = 10\nsum(3, 4) = 7\nhello arrow\nanswer = 42\ntriple(7) = 21\n");
  });
});
