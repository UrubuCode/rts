import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

const greet = function(name: string): void {
  print(`hello ${name}`);
};

const double = function doubleImpl(n: i32): i32 {
  return n * 2;
};

const triple = (n: i32): i32 => {
  return n * 3;
};

greet("world");
greet("RTS");
print(`double(5) = ${double(5)}`);
print(`triple(7) = ${triple(7)}`);

describe("fixture:function_expressions", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("hello world\nhello RTS\ndouble(5) = 10\ntriple(7) = 21\n");
  });
});
