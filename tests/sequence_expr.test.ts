import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Comma operator (sequence expression) — evaluates all, returns last
const x = (1, 2, 3);
print(`${x}`);

// Side effects with comma
let a = 0;
const y = (a++, a++, a);
print(`${y}`);
print(`${a}`);

// In for loop update
let sum = 0;
for (let i = 0, j = 10; i < 5; i++, j--) {
  sum += i + j;
}
print(`${sum}`);

// Comma in condition (less common but valid)
function sideEffect(): number {
  a += 10;
  return a;
}
const z = (sideEffect(), sideEffect(), a);
print(`${z}`);

describe("sequence_expr", () => {
  test("comma_op", () => expect(__rtsCapturedOutput).toBe("3\n2\n2\n70\n22\n"));
});
