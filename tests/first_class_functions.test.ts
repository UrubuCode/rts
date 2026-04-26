import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

function double(x: i32): i32 { return x * 2; }
function triple(x: i32): i32 { return x * 3; }
function addOne(x: i32): i32 { return x + 1; }

// Função que recebe outro funcptr como primeiro argumento e chama.
function apply(fn: i64, x: i32): i32 { return fn(x); }

// Higher-order: compõe dois funcptrs em um valor f(g(x)).
function compose2(f: i64, g: i64, x: i32): i32 {
  return f(g(x));
}

print(`${apply(double, 5)}`);
print(`${apply(triple, 5)}`);
print(`${apply(addOne, 9)}`);
print(`${compose2(double, addOne, 4)}`);

describe("fixture:first_class_functions", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("10\n15\n10\n10\n");
  });
});
