// purity_pass + reduce_pass agora cobrem for...of dentro de fns
// (antes so top-level). Pedaco B do epic #247.
import { describe, test, expect } from "rts:test";
import { gc, math } from "rts";

// for...of dentro de fn — purity_pass deve detectar e reescrever
// para parallel.for_each.
function callMath(arr: i64): i64 {
  for (const x of [1, 2, 3, 4, 5]) {
    const _ = math.sin(x as number);
    const _2 = math.sqrt((x * 2) as number);
  }
  return 0;
}

// Reduce dentro de fn — reduce_pass detecta acc + x.
function sumOf5(): i64 {
  let s = 0;
  for (const x of [10, 20, 30, 40, 50]) {
    s = s + x;
  }
  return s;
}

// Reduce com fn pura no body
function sumAbs(): i64 {
  let s = 0;
  for (const x of [-1, -2, -3, -4, -5]) {
    s = s + math.abs_i64(x);
  }
  return s;
}

const m = callMath(0);
const sum = sumOf5();
const a = sumAbs();

describe("fixture:parallel_purity_in_fn", () => {
  test("for...of pura dentro de fn nao crasha (purity_pass)", () => {
    expect(gc.string_from_i64(m)).toBe("0");
  });

  test("reduce dentro de fn — sum 10+20+30+40+50 = 150", () => {
    expect(gc.string_from_i64(sum)).toBe("150");
  });

  test("reduce dentro de fn com fn pura — sum abs = 15", () => {
    expect(gc.string_from_i64(a)).toBe("15");
  });
});
