import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Anonymous function expression
const double = function(x: number): number { return x * 2; };
print(`${double(5)}`);

// Named function expression (name only visible inside)
const factorial = function fact(n: number): number {
  return n <= 1 ? 1 : n * fact(n - 1);
};
print(`${factorial(5)}`);

// Function expression as argument
function apply(f: (x: number) => number, x: number): number {
  return f(x);
}
print(`${apply(function(x) { return x + 10; }, 5)}`);

// IIFE (immediately invoked function expression)
const result = (function(a: number, b: number) { return a + b; })(3, 4);
print(`${result}`);

// Function expression in object
const math = {
  add: function(a: number, b: number): number { return a + b; },
  mul: function(a: number, b: number): number { return a * b; },
};
print(`${math.add(2, 3)}`);
print(`${math.mul(4, 5)}`);

describe("function_expression", () => {
  test("basic", () => expect(__rtsCapturedOutput).toBe("10\n120\n15\n7\n5\n20\n"));
});
