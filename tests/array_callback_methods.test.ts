import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const nums = [1, 2, 3, 4, 5];

const doubled = nums.map(x => x * 2);
print(doubled.join(","));  // "2,4,6,8,10"

const sum = nums.reduce((a, b) => a + b, 0);
print(`${sum}`);  // "15"

const product = nums.reduce((a, b) => a * b, 1);
print(`${product}`);  // "120"

// Variant: arrow with explicit BlockStmt + return
const tripled = nums.map(x => { return x * 3; });
print(tripled.join(","));  // "3,6,9,12,15"

// Variant: arrow com paren em torno do param.
const squared = nums.map((x) => x * x);
print(squared.join(","));  // "1,4,9,16,25"

describe("array_callback_methods", () => {
  test("map with arrow", () => expect(doubled.join(",")).toBe("2,4,6,8,10"));
  test("reduce sum", () => expect(`${sum}`).toBe("15"));
  test("reduce product", () => expect(`${product}`).toBe("120"));
  test("map with block-return arrow", () => expect(tripled.join(",")).toBe("3,6,9,12,15"));
  test("map with paren param arrow", () => expect(squared.join(",")).toBe("1,4,9,16,25"));
  test("captured output", () => expect(out).toBe(
    "2,4,6,8,10\n15\n120\n3,6,9,12,15\n1,4,9,16,25\n"
  ));
});
