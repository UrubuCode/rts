import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Labeled break: sai do loop externo a partir do interno.

let pairs: number = 0;
outer: for (let i = 0; i < 5; i = i + 1) {
    for (let j = 0; j < 5; j = j + 1) {
        if (i == 2 && j == 3) {
            break outer;
        }
        pairs = pairs + 1;
    }
}

const h = gc.string_from_i64(pairs);
print(h); gc.string_free(h); // 5+5+3 = 13

describe("fixture:labeled_break", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("13\n");
  });
});
