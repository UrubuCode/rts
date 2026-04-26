import { describe, test, expect } from "rts:test";
import { io, gc, math } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Default que é expressão composta + chamada.

function compute(x: number = math.abs_i64(-5)): number {
    return x * 2;
}

const h1 = gc.string_from_i64(compute());      // 5 * 2 = 10
print(h1); gc.string_free(h1);

const h2 = gc.string_from_i64(compute(7));     // 14
print(h2); gc.string_free(h2);

describe("fixture:default_param_expr", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("10\n14\n");
  });
});
