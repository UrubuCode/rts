import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

print(`${Number.MAX_SAFE_INTEGER}`);
print(`${Number.MIN_SAFE_INTEGER}`);
print(`${Number.POSITIVE_INFINITY}`);
print(`${Number.NEGATIVE_INFINITY}`);
print(`${Number.NaN}`);

if (Number.MAX_SAFE_INTEGER > 0) {
  print("positive");
}

describe("number_constants", () => {
  test("globals_match_js", () => expect(__rtsCapturedOutput).toBe(
    "9007199254740991\n-9007199254740991\nInfinity\n-Infinity\nNaN\npositive\n"
  ));
});
