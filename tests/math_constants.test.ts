import { describe, test, expect } from "rts:test";

let out = "";

// (#208) Math constants — JS spec.
out += Math.PI + "\n";
out += Math.E + "\n";
out += Math.SQRT2 + "\n";
out += Math.SQRT1_2 + "\n";
out += Math.LN2 + "\n";
out += Math.LN10 + "\n";
out += Math.LOG2E + "\n";
out += Math.LOG10E + "\n";

describe("math_constants", () => {
  test("PI/E/SQRT2/SQRT1_2/LN2/LN10/LOG2E/LOG10E (#208)", () => expect(out).toBe(
    "3.141592653589793\n2.718281828459045\n1.4142135623730951\n0.7071067811865476\n0.6931471805599453\n2.302585092994046\n1.4426950408889634\n0.4342944819032518\n"
  ));
});
