import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

const nums = [10, 20, 30, 40];
let sum: i32 = 0;
for (const n of nums) {
  sum += n;
}
print(`sum=${sum}`);

const matrix = [[1, 2], [3, 4]];
for (const row of matrix) {
  for (const x of row) {
    print(`x=${x}`);
  }
}

const words = ["foo", "bar", "foo"];
let foos: i32 = 0;
let w: string = "";
for (w of words) {
  if (w == "foo") {
    foos += 1;
  }
}
print(`foos=${foos}`);

const objs = [{ v: 100 }, { v: 200 }, { v: 300 }];
let total: i32 = 0;
for (const obj of objs) {
  total += obj.v;
}
print(`total=${total}`);

describe("fixture:for_of", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("sum=100\nx=1\nx=2\nx=3\nx=4\nfoos=2\ntotal=600\n");
  });
});
