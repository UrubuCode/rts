import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Spread de array literal em chamada de fn user.

function add3(a: number, b: number, c: number): number {
    return a + b + c;
}

const h = gc.string_from_i64(add3(...[1, 2, 3]));
print(h); gc.string_free(h); // 6

describe("fixture:spread_call_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("6\n");
  });
});
