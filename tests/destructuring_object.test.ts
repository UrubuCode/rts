import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Basic object destructuring
const { x, y } = { x: 10, y: 20 };
print(`${x} ${y}`);

// Rename binding
const { a: renamed } = { a: 42 };
print(`${renamed}`);

// Default values
const { m = 5, n = 10 } = { m: 1 };
print(`${m} ${n}`);

// Nested object destructuring
const { outer: { inner } } = { outer: { inner: 99 } };
print(`${inner}`);

// Rest in object destructuring
const { p, ...remaining } = { p: 1, q: 2, r: 3 };
print(`${p}`);

// In function parameter
function greet({ name, age }: { name: string; age: number }): void {
  print(`${name} is ${age}`);
}
greet({ name: "Alice", age: 30 });

describe("destructuring_object", () => {
  test("basic", () => expect(__rtsCapturedOutput).toBe("10 20\n42\n1 10\n99\n1\nAlice is 30\n"));
});
