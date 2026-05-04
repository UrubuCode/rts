import { describe, test, expect } from "rts:test";

let out = "";

// (#208) Array.indexOf(needle, fromIndex)
const a = [1, 2, 3, 2, 1];
out += a.indexOf(2) + "\n";       // 1
out += a.indexOf(2, 2) + "\n";    // 3
out += a.indexOf(1, 1) + "\n";    // 4
out += a.indexOf(99, 0) + "\n";   // -1
out += a.indexOf(2, -2) + "\n";   // 3 (negativo: -2 → 3)

describe("array_indexof_fromindex", () => {
  test("indexOf 2-arg (#208)", () => expect(out).toBe(
    "1\n3\n4\n-1\n3\n"
  ));
});
