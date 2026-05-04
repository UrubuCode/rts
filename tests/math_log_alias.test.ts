import { describe, test, expect } from "rts:test";

let out = "";

// (#208) Math.log = ln (natural log) — JS spec.
out += Math.log(Math.E) + "\n";   // 1
out += Math.log(1) + "\n";        // 0

// log2 e log10 ja existiam.
out += Math.log2(8) + "\n";       // 3
out += Math.log10(1000) + "\n";   // 3

describe("math_log_alias", () => {
  test("Math.log alias de ln (#208)", () => expect(out).toBe(
    "1\n0\n3\n3\n"
  ));
});
