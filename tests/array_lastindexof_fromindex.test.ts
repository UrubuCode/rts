import { describe, test, expect } from "rts:test";

let out = "";

// (#208) Array.lastIndexOf(needle, fromIndex)
const a = [1, 2, 3, 2, 1];
out += a.lastIndexOf(2) + "\n";       // 3
out += a.lastIndexOf(2, 2) + "\n";    // 1
out += a.lastIndexOf(1, 0) + "\n";    // 0

describe("array_lastindexof_fromindex", () => {
  test("lastIndexOf 2-arg (#208)", () => expect(out).toBe(
    "3\n1\n0\n"
  ));
});
