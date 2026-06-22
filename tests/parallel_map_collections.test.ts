import { describe, test, expect } from "rts:test";
import { math, parallel, collections } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 1) parallel.map result is a fully-functional Vec (uses sharded handle table).
function cube(x: number): number {
  return x * x * x;
}
const cfp = getPointer(cube);
const nums = [1, 2, 3, 4, 5];
const cubed = parallel.map(nums, cfp);

print(`${collections.vec_len(cubed)}`); // 5

print(`${collections.vec_get(cubed, 0)}`); // 1

print(`${collections.vec_get(cubed, 2)}`); // 27

print(`${collections.vec_get(cubed, 4)}`); // 125

// 2) parallel.map on a Vec created by vec_new + vec_push.
const h = collections.vec_new();
collections.vec_push(h, 10);
collections.vec_push(h, 20);
collections.vec_push(h, 30);

function halve(x: number): number {
  return x / 2;
}
const hfp = getPointer(halve);
const halved = parallel.map(h, hfp);

print(`${collections.vec_get(halved, 0)}`); // 5

print(`${collections.vec_get(halved, 2)}`); // 15

// 3) parallel.reduce after parallel.map (chain of parallel ops).
function sumTwo(acc: number, x: number): number {
  return acc + x;
}
const sfp = getPointer(sumTwo);
const total = parallel.reduce(cubed, 0, sfp);
print(`${total}`); // 1+8+27+64+125 = 225

// 4) parallel.map empty vec → vec_len = 0.
const empty = collections.vec_new();
function identity(x: number): number { return x; }
const ifp = getPointer(identity);
const mapped_empty = parallel.map(empty, ifp);
print(`${collections.vec_len(mapped_empty)}`); // 0

// 5) vec_set on parallel.map result.
collections.vec_set(cubed, 0, 999);
print(`${collections.vec_get(cubed, 0)}`); // 999

describe("fixture:parallel_map_collections", () => {
  test("parallel.map integrates with sharded HandleTable", () => {
    expect(__rtsCapturedOutput).toBe(
      "5\n1\n27\n125\n5\n15\n225\n0\n999\n"
    );
  });
});
