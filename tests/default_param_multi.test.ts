import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Múltiplos defaults; chamadas com 1, 2 ou 3 args.

function combine(a: number, b: number = 10, c: number = 100): number {
    return a + b + c;
}

const h1 = gc.string_from_i64(combine(1));         // 1+10+100 = 111
print(h1); gc.string_free(h1);

const h2 = gc.string_from_i64(combine(1, 2));      // 1+2+100 = 103
print(h2); gc.string_free(h2);

const h3 = gc.string_from_i64(combine(1, 2, 3));   // 6
print(h3); gc.string_free(h3);

describe("fixture:default_param_multi", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("111\n103\n6\n");
  });
});
