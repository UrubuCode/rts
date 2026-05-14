// Object.is distingue 0 e -0 (#872 fixture 134).
import { describe, test, expect } from "rts:test";

let out = "";
out += "is_0_neg0=" + Object.is(0, -0) + "\n";   // false
out += "eq_0_neg0=" + (0 === -0) + "\n";          // true (=== nao distingue)
out += "is_NaN_NaN=" + Object.is(NaN, NaN) + "\n"; // true
out += "eq_NaN_NaN=" + (NaN === NaN) + "\n";       // false
out += "is_5_5=" + Object.is(5, 5) + "\n";         // true
out += "is_5_str=" + Object.is(5, "5") + "\n";     // false

describe("Object.is -0/0 distinction (#872)", () => {
  test("egal semantics", () => expect(out).toBe(
    "is_0_neg0=false\neq_0_neg0=true\nis_NaN_NaN=true\neq_NaN_NaN=false\nis_5_5=true\nis_5_str=false\n"
  ));
});
