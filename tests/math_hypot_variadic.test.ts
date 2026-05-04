import { describe, test, expect } from "rts:test";

let out = "";

// (#208) Math.hypot(...args) — N args.
out += Math.hypot(3, 4) + "\n";        // 5
out += Math.hypot(3, 4, 12) + "\n";    // 13 (sqrt(9+16+144))
out += Math.hypot(0, 0) + "\n";        // 0
out += Math.hypot(1, 1, 1, 1) + "\n";  // 2 (sqrt(4))

describe("math_hypot_variadic", () => {
  test("Math.hypot N args (#208)", () => expect(out).toBe(
    "5\n13\n0\n2\n"
  ));
});
