import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

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

const a = gc.string_from_i64(Code.Ok);
print(a); gc.string_free(a);
const b = gc.string_from_i64(Code.NotFound);
print(b); gc.string_free(b);
print(Code.Banner);

describe("fixture:enum_mixed", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("200\n404\n*** atencao ***\n");
  });
});
