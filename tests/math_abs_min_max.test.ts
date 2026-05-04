import { describe, test, expect } from "rts:test";

let out = "";

// (#208) Math.abs/min/max aliases JS-style (sem _f64).
out += Math.abs(-5) + "\n";       // 5
out += Math.abs(3.14) + "\n";     // 3.14

out += Math.min(1, 5) + "\n";     // 1
out += Math.max(1, 5) + "\n";     // 5

out += Math.min(2.5, 1.5) + "\n"; // 1.5
out += Math.max(2.5, 1.5) + "\n"; // 2.5

describe("math_abs_min_max", () => {
  test("aliases sem sufixo (#208)", () => expect(out).toBe(
    "5\n3.14\n1\n5\n1.5\n2.5\n"
  ));
});
