import { describe, test, expect } from "rts:test";

let out = "";

// (#208) String.fromCodePoint variadico.
out += String.fromCodePoint(72, 105) + "\n";   // Hi
out += String.fromCodePoint(82, 84, 83) + "\n"; // RTS
out += String.fromCodePoint(0x1F600) + "\n";    // 😀 (emoji 4-byte)

describe("string_from_code_point_multi", () => {
  test("fromCodePoint variadico (#208)", () => expect(out).toBe(
    "Hi\nRTS\n😀\n"
  ));
});
