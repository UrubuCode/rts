import { describe, test, expect } from "rts:test";

let out = "";

// (#208) Math.max/min variadico — JS spec aceita N args.
out += Math.max(1, 5, 3, 8, 2) + "\n";   // 8
out += Math.min(1, 5, 3, 8, 2) + "\n";   // 1
out += Math.max(1, 2) + "\n";            // 2 (2-arg ainda funciona)
out += Math.min(1, 2) + "\n";            // 1
out += Math.max(10, 20, 30) + "\n";      // 30

describe("math_max_min_variadic", () => {
  test("Math.max/min com N args (#208)", () => expect(out).toBe(
    "8\n1\n2\n1\n30\n"
  ));
});
