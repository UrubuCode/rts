import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Union com null: aceita null como sentinel (handle 0).

function maybe(x: number): number | null {
    if (x > 0) { return x; }
    return null;
}

const a = maybe(5);
print(`${a as number}`); // 5

const b = maybe(-1);
// b é null ≡ 0 em RTS
print(`${b as number}`); // 0

describe("fixture:union_null", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("5\n0\n");
  });
});
