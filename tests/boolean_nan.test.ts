import { describe, test, expect } from "rts:test";

let out = "";

// (#208 fix): Boolean(NaN) deve ser false (NaN e' falsy em JS).
out += (Boolean(0) ? "y" : "n") + "\n";       // n
out += (Boolean(1) ? "y" : "n") + "\n";       // y
out += (Boolean(NaN) ? "y" : "n") + "\n";     // n (era y antes do fix)
out += (Boolean(-0) ? "y" : "n") + "\n";      // n
out += (Boolean(Infinity) ? "y" : "n") + "\n";// y
out += (Boolean(3.14) ? "y" : "n") + "\n";    // y

describe("boolean_nan", () => {
  test("Boolean(NaN) === false (#208 fix)", () => expect(out).toBe(
    "n\ny\nn\nn\ny\ny\n"
  ));
});
