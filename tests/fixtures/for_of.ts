import { io } from "rts";

const nums = [10, 20, 30, 40];
let sum: i32 = 0;
for (const n of nums) {
  sum += n;
}
io.print(`sum=${sum}`);

const matrix = [[1, 2], [3, 4]];
for (const row of matrix) {
  for (const x of row) {
    io.print(`x=${x}`);
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
io.print(`foos=${foos}`);

const objs = [{ v: 100 }, { v: 200 }, { v: 300 }];
let total: i32 = 0;
for (const obj of objs) {
  total += obj.v;
}
io.print(`total=${total}`);
