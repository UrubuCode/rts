import { describe, test, expect } from "rts:test";

let out = "";

// Math.sign
out += Math.sign(-5) + "\n";
out += Math.sign(5) + "\n";
out += Math.sign(0) + "\n";

// Math.hypot
out += Math.hypot(3, 4) + "\n";  // 5

// Math.expm1, log1p
out += Math.expm1(0) + "\n";     // 0
out += Math.log1p(0) + "\n";     // 0

// Hyperbolic trig
out += Math.sinh(0) + "\n";      // 0
out += Math.cosh(0) + "\n";      // 1
out += Math.tanh(0) + "\n";      // 0

describe("math_extras", () => {
  test("sign/hypot/expm1/log1p/sinh/cosh/tanh", () => expect(out).toBe(
    "-1\n1\n0\n5\n0\n0\n0\n1\n0\n"
  ));
});
