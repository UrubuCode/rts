import { describe, test, expect } from "rts:test";
import { io, gc, regex } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Regex replace + replace_all.

const foo = /foo/;
const h1 = regex.replace_all(foo, "foo bar foo baz", "X");
print(h1); gc.string_free(h1); // X bar X baz
const h2 = regex.replace(foo, "foo and foo", "Y");
print(h2); gc.string_free(h2); // Y and foo
regex.free(foo);

describe("fixture:regex_replace", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("X bar X baz\nY and foo\n");
  });
});
