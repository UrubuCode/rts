import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Numeric enum básico, auto-incremento de 0.

enum Status {
    Pending,
    Active,
    Closed,
}

const h0 = gc.string_from_i64(Status.Pending);
print(h0); gc.string_free(h0); // 0
const h1 = gc.string_from_i64(Status.Active);
print(h1); gc.string_free(h1); // 1
const h2 = gc.string_from_i64(Status.Closed);
print(h2); gc.string_free(h2); // 2

describe("fixture:enum_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("0\n1\n2\n");
  });
});
