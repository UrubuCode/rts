import { describe, test, expect } from "rts:test";

let out = "";

// (#208) String.fromCharCode com multiplos args.
out += String.fromCharCode(72, 105) + "\n";              // Hi
out += String.fromCharCode(82, 84, 83) + "\n";           // RTS
out += String.fromCharCode(104, 101, 108, 108, 111) + "\n";  // hello
out += String.fromCharCode(65) + "\n";                   // A (1 arg ainda funciona)

describe("string_from_char_code_multi", () => {
  test("fromCharCode variadico (#208)", () => expect(out).toBe(
    "Hi\nRTS\nhello\nA\n"
  ));
});
