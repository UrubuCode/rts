import { describe, test, expect } from "rts:test";
import { io, gc, ptr } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// ptr.null/is_null/offset.

const p = ptr.null();
const h1 = gc.string_from_i64(p);
print(h1); gc.string_free(h1); // 0

const isNull = ptr.is_null(0) ? 1 : 0;
const h2 = gc.string_from_i64(isNull);
print(h2); gc.string_free(h2); // 1

const isNotNull = ptr.is_null(0x1000) ? 1 : 0;
const h3 = gc.string_from_i64(isNotNull);
print(h3); gc.string_free(h3); // 0

const off = ptr.offset(0x1000, 16);
const h4 = gc.string_from_i64(off);
print(h4); gc.string_free(h4); // 4112

describe("fixture:ptr_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("0\n1\n0\n4112\n");
  });
});
