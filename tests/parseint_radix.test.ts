import { describe, test, expect } from "rts:test";

let out = "";

// (#208) parseInt JS-spec: radix opcional, tolerante a trailing chars.
out += parseInt("100") + "\n";        // 100 (default 10)
out += parseInt("100", 10) + "\n";    // 100
out += parseInt("FF", 16) + "\n";     // 255
out += parseInt("ff", 16) + "\n";     // 255
out += parseInt("0xFF") + "\n";       // 255 (auto-detect 0x → 16)
out += parseInt("101", 2) + "\n";     // 5
out += parseInt("777", 8) + "\n";     // 511
out += parseInt("-42") + "\n";        // -42
out += parseInt("42abc") + "\n";      // 42 (tolerante)
out += parseInt("  +7  ", 10) + "\n"; // 7 (whitespace)

describe("parseint_radix", () => {
  test("parseInt JS-spec com radix (#208)", () => expect(out).toBe(
    "100\n100\n255\n255\n255\n5\n511\n-42\n42\n7\n"
  ));
});
