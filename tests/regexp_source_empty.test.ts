// (#877) RegExp.prototype.source — empty pattern returns "(?:)".
import { describe, test, expect } from "rts:test";

let out = "";
out += "abc=" + (/abc/).source + "\n";
out += "empty=" + (new RegExp("")).source + "\n";
out += "digits=" + (new RegExp("\\d+")).source + "\n";

describe("RegExp.source empty (#877)", () => {
  test("empty pattern returns (?:)", () => expect(out).toBe(
    "abc=abc\nempty=(?:)\ndigits=\\d+\n"
  ));
});
