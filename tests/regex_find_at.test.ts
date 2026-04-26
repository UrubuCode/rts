import { describe, test, expect } from "rts:test";
import { io, gc, regex } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Regex find_at + match_count.

const word = /[0-9]+/;
const idx = regex.find_at(word, "abc 123 def 456");
const cnt = regex.match_count(word, "abc 123 def 456");
const h1 = gc.string_from_i64(idx);
print(h1); gc.string_free(h1); // 4
const h2 = gc.string_from_i64(cnt);
print(h2); gc.string_free(h2); // 2
regex.free(word);

describe("fixture:regex_find_at", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("4\n2\n");
  });
});
