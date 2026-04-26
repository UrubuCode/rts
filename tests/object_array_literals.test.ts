import { describe, test, expect } from "rts:test";
import { io, collections } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

const nums = [10, 20, 30];
print(`vec_len=${collections.vec_len(nums)}`);
print(`nums[0]=${nums[0]}`);
print(`nums[2]=${nums[2]}`);

const point = { x: 1, y: 2 };
print(`point.x=${point.x} point.y=${point.y}`);
print(`point["y"]=${point["y"]}`);

const x = 7;
const y = 8;
const sh = { x, y };
print(`sh.x=${sh.x} sh.y=${sh.y}`);

const matrix = [[1, 2], [3, 4]];
const row0 = matrix[0];
const row1 = matrix[1];
print(`matrix[0][0]=${row0[0]} matrix[1][1]=${row1[1]}`);

const objs = [{ v: 100 }, { v: 200 }];
print(`objs[0].v=${objs[0].v}`);
print(`objs[1].v=${objs[1].v}`);

const data = { items: [11, 22, 33] };
const items = data.items;
print(`data.items[1]=${items[1]}`);

describe("fixture:object_array_literals", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("vec_len=3\nnums[0]=10\nnums[2]=30\npoint.x=1 point.y=2\npoint[\"y\"]=2\nsh.x=7 sh.y=8\nmatrix[0][0]=1 matrix[1][1]=4\nobjs[0].v=100\nobjs[1].v=200\ndata.items[1]=22\n");
  });
});
