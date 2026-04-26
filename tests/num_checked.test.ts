import { describe, test, expect } from "rts:test";
import { io, gc, num } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// num.checked_*: aritmetica que sinaliza overflow via i64::MIN.

const a = num.checked_add(100, 200);
const h1 = gc.string_from_i64(a); print(h1); gc.string_free(h1);

const b = num.checked_div(100, 0);
const h2 = gc.string_from_i64(b); print(h2); gc.string_free(h2);

const c = num.checked_sub(50, 30);
const h3 = gc.string_from_i64(c); print(h3); gc.string_free(h3);

const d = num.checked_mul(7, 6);
const h4 = gc.string_from_i64(d); print(h4); gc.string_free(h4);

describe("fixture:num_checked", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("300\n-9223372036854775808\n20\n42\n");
  });
});
