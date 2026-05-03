import { describe, test, expect } from "rts:test";

let out = "";

// sort sem comparator (ascendente)
const a = [3, 1, 4, 1, 5, 9, 2, 6];
a.sort();
out += a.join(",") + "\n";    // 1,1,2,3,4,5,6,9

// findLast / findLastIndex (predicate retorna i64 0/1)
function gtTwo(x: number): number { return x > 2 ? 1 : 0; }
const c = [1, 3, 5, 2, 4];
out += c.findLast(gtTwo) + "\n";        // 4
out += c.findLastIndex(gtTwo) + "\n";   // 4

// copyWithin
const cw = [1, 2, 3, 4, 5];
cw.copyWithin(0, 3);
out += cw.join(",") + "\n";   // 4,5,3,4,5

describe("array_more_methods", () => {
  test("sort/findLast/findLastIndex/copyWithin", () => expect(out).toBe(
    "1,1,2,3,4,5,6,9\n4\n4\n4,5,3,4,5\n"
  ));
});
