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

// Rest in object destructuring (#312)
const { p, ...remaining } = { p: 1, q: 2, r: 3 };
print(`${p}`);
print(`${remaining.q} ${remaining.r}`);

// In function parameter — tipos numericos (string em parametro destructured
// depende de fix preexistente em concatenacao de template com handle).
function addPair({ left, right }: { left: number; right: number }): void {
  print(`${left + right}`);
}
addPair({ left: 30, right: 12 });

describe("destructuring_object", () => {
  test("basic", () => expect(__rtsCapturedOutput).toBe("10 20\n42\n1 10\n99\n1\n2 3\n42\n"));
});
