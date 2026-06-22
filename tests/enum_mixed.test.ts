import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Enum misto: numeric e string convivendo (TS permite).

enum Code {
  Ok = 200,
  NotFound = 404,
  Banner = "*** atencao ***",
}

print(`${Code.Ok}`);
print(`${Code.NotFound}`);
print(Code.Banner);

describe("fixture:enum_mixed", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("200\n404\n*** atencao ***\n");
  });
});
