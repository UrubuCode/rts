import { describe, test, expect } from "rts:test";
import { io, regex } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Regex find_at + match_count.

const word = /[0-9]+/;
const idx = regex.find_at(word, "abc 123 def 456");
const cnt = regex.match_count(word, "abc 123 def 456");
print(`${idx}`); // 4
print(`${cnt}`); // 2
regex.free(word);

describe("fixture:regex_find_at", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("4\n2\n");
  });
});
