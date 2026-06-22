import { describe, test, expect } from "rts:test";
import { io, regex } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Regex literal + test().

const re = /^[a-z]+@[a-z]+\.[a-z]+$/i;
const ok = regex.test(re, "USER@MAIL.COM") ? 1 : 0;
const bad = regex.test(re, "not-an-email") ? 1 : 0;
print(`${ok}`); // 1
print(`${bad}`); // 0
regex.free(re);

describe("fixture:regex_test", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("1\n0\n");
  });
});
