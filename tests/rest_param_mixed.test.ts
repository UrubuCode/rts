import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Mistura param normal + rest.

function joinFrom(start: number, ...extra: number[]): number {
    let total = start;
    for (const n of extra) {
        total = total + n;
    }
    return total;
}

const h1 = gc.string_from_i64(joinFrom(100, 1, 2, 3));
print(h1); gc.string_free(h1); // 106
const h2 = gc.string_from_i64(joinFrom(50));
print(h2); gc.string_free(h2); // 50

describe("fixture:rest_param_mixed", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("106\n50\n");
  });
});
