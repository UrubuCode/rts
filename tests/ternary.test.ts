import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

const x = 10;
const label = x > 5 ? "big" : "small";
print(label);

const abs = x < 0 ? -x : x;
print(`abs = ${abs}`);

const n = 0;
const sign = n > 0 ? "pos" : n < 0 ? "neg" : "zero";
print(sign);

function half(v: i32): i32 { return v / 2; }
const y = true ? half(20) : 0;
print(`y = ${y}`);

describe("fixture:ternary", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("big\nabs = 10\nzero\ny = 10\n");
  });
});
