import { describe, test, expect } from "rts:test";

let out = "";

// (#208) Boolean.prototype.toString — true/false como strings.
const t: boolean = true;
const f: boolean = false;
out += t.toString() + "\n";  // true
out += f.toString() + "\n";  // false

describe("boolean_tostring", () => {
  test("Boolean.toString (#208)", () => expect(out).toBe("true\nfalse\n"));
});
