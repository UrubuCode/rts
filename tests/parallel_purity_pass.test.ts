import { describe, test, expect } from "rts:test";
import { math, parallel, collections, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 1) parallel.num_threads() > 0 (Rayon pool alive)
const nt = parallel.num_threads();
if (nt > 0) {
  print("num-threads-ok");
} else {
  print("FAIL: num_threads=0");
}

// 2) Pure for...of: body only reads loop var + calls pure math fns.
//    Purity pass should rewrite to parallel.for_each. No crash = pass.
const angles = [0, 1, 2, 3, 4, 5, 6, 7];
for (const x of angles) {
  const s = math.sin(x as number);
  const c = math.cos(s);
}
print("pure-forof-ok");

// 3) Manual parallel.for_each with a top-level function.
function doubleWorker(x: number): void {
  // reads x, pure computation — just validate it runs
  const _ = x * 2;
}
const dfp = getPointer(doubleWorker);
const nums = [10, 20, 30, 40];
parallel.for_each(nums, dfp);
print("manual-foreach-ok");

// 4) parallel.map: results collected correctly.
function squareFn(x: number): number {
  return x * x;
}
const sfp = getPointer(squareFn);
const src = [1, 2, 3, 4, 5];
const squared = parallel.map(src, sfp);
const len = collections.vec_len(squared);
const h = gc.string_from_i64(len);
print(h); gc.string_free(h);   // expect 5

const v0 = collections.vec_get(squared, 0);
const hv0 = gc.string_from_i64(v0);
print(hv0); gc.string_free(hv0); // expect 1

const v4 = collections.vec_get(squared, 4);
const hv4 = gc.string_from_i64(v4);
print(hv4); gc.string_free(hv4); // expect 25

// 5) parallel.reduce: sum of [1..5] = 15
function addFn(acc: number, x: number): number {
  return acc + x;
}
const afp = getPointer(addFn);
const total = parallel.reduce(src, 0, afp);
const ht = gc.string_from_i64(total);
print(ht); gc.string_free(ht); // expect 15

// 6) Multiple pure for...of loops (each gets own __par_forof_N).
const vals = [100, 200, 300];
for (const v of vals) {
  const norm = v as number / 100.0;
  const sq = math.sqrt(norm);
}
for (const v of vals) {
  const lg = math.log2(v as number);
}
print("multi-forof-ok");

describe("fixture:parallel_purity_pass", () => {
  test("parallel infrastructure + purity pass smoke", () => {
    expect(__rtsCapturedOutput).toBe(
      "num-threads-ok\npure-forof-ok\nmanual-foreach-ok\n5\n1\n25\n15\nmulti-forof-ok\n"
    );
  });
});
