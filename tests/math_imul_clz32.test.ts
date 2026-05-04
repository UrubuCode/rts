import { describe, test, expect } from "rts:test";

let out = "";

// (#208) Math.imul — C-style 32-bit signed multiplication.
out += Math.imul(3, 4) + "\n";       // 12
out += Math.imul(0xFFFFFFFF, 5) + "\n";  // -5 (wrapping)

// (#208) Math.clz32 — count leading zeros em uint32.
out += Math.clz32(1) + "\n";         // 31
out += Math.clz32(0) + "\n";         // 32
out += Math.clz32(0xFFFFFFFF) + "\n";// 0

describe("math_imul_clz32", () => {
  test("imul + clz32 (#208)", () => expect(out).toBe(
    "12\n-5\n31\n32\n0\n"
  ));
});
