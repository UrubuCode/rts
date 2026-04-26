import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Default parameter: chamada sem o arg usa o default.

function add(a: number, b: number = 10): number {
    return a + b;
}

const h1 = gc.string_from_i64(add(5));
print(h1); gc.string_free(h1); // 15 (5 + default 10)

const h2 = gc.string_from_i64(add(5, 100));
print(h2); gc.string_free(h2); // 105

describe("fixture:default_param_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("15\n105\n");
  });
});
