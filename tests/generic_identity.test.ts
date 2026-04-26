import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Generic function: identity<T>.

function identity<T>(x: T): T {
  return x;
}

const a = identity<i64>(42);
const h = gc.string_from_i64(a);
print(h); gc.string_free(h);

const b = identity<i64>(-7);
const h2 = gc.string_from_i64(b);
print(h2); gc.string_free(h2);

describe("fixture:generic_identity", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("42\n-7\n");
  });
});
