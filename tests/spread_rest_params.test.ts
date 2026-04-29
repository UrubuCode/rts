import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Rest parameters
function sum(...args: number[]): number {
  let total = 0;
  for (const n of args) total += n;
  return total;
}
print(`${sum(1, 2, 3)}`);
print(`${sum(10, 20)}`);

// Rest with leading params
function first(a: number, b: number, ...rest: number[]): void {
  print(`${a} ${b} rest=${rest.length}`);
}
first(1, 2, 3, 4, 5);

// Spread in call site
function add(a: number, b: number, c: number): number {
  return a + b + c;
}
const args = [1, 2, 3];
print(`${add(...args)}`);

// Spread + fixed args
print(`${add(0, ...([2, 3]))}`);

// Spread in array literal
const a = [1, 2];
const b = [3, 4];
const merged = [...a, ...b];
print(`${merged.length}`);

describe("spread_rest_params", () => {
  test("rest", () => expect(__rtsCapturedOutput).toBe("6\n30\n1 2 rest=3\n6\n5\n4\n"));
});
