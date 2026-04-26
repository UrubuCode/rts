import { describe, test, expect } from "rts:test";
import { io, gc, num } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// wrapping_*: aritmetica modular.

// MAX + 1 -> MIN (modular wrap).
const a = num.wrapping_add(9223372036854775807, 1);
const h1 = gc.string_from_i64(a); print(h1); gc.string_free(h1);

// 0 - 1 -> -1
const b = num.wrapping_sub(0, 1);
const h2 = gc.string_from_i64(b); print(h2); gc.string_free(h2);

const c = num.wrapping_neg(42);
const h3 = gc.string_from_i64(c); print(h3); gc.string_free(h3);

const d = num.wrapping_shl(1, 4);
const h4 = gc.string_from_i64(d); print(h4); gc.string_free(h4);

const e = num.wrapping_shr(256, 4);
const h5 = gc.string_from_i64(e); print(h5); gc.string_free(h5);

describe("fixture:num_wrapping", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("-9223372036854775808\n-1\n-42\n16\n16\n");
  });
});
