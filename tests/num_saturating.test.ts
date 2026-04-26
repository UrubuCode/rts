import { describe, test, expect } from "rts:test";
import { io, gc, num } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// saturating_*: clamp em i64::MIN/MAX.

// 9_000_000_000_000_000_000 + 9_000_000_000_000_000_000 saturaria.
const a = num.saturating_add(9000000000000000000, 9000000000000000000);
const h1 = gc.string_from_i64(a); print(h1); gc.string_free(h1);

const b = num.saturating_sub(-9000000000000000000, 9000000000000000000);
const h2 = gc.string_from_i64(b); print(h2); gc.string_free(h2);

const c = num.saturating_mul(3, 7);
const h3 = gc.string_from_i64(c); print(h3); gc.string_free(h3);

describe("fixture:num_saturating", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("9223372036854775807\n-9223372036854775808\n21\n");
  });
});
