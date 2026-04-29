import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Basic array destructuring
const [a, b, c] = [1, 2, 3];
print(`${a} ${b} ${c}`);

// With skipped elements
const [x, , z] = [10, 20, 30];
print(`${x} ${z}`);

// With default values
const [p = 5, q = 9] = [1];
print(`${p} ${q}`);

// Nested
const [[inner1, inner2], outer] = [[1, 2], 3];
print(`${inner1} ${inner2} ${outer}`);

// Rest element
const [first, ...rest] = [1, 2, 3, 4];
print(`${first} ${rest.length}`);

describe("destructuring_array", () => {
  test("basic", () => expect(__rtsCapturedOutput).toBe("1 2 3\n10 30\n1 9\n1 2 3\n1 3\n"));
});
