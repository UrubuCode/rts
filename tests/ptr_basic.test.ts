import { describe, test, expect } from "rts:test";
import { io, ptr } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// ptr.null/is_null/offset.

const p = ptr.null();
print(`${p}`); // 0

const isNull = ptr.is_null(0) ? 1 : 0;
print(`${isNull}`); // 1

const isNotNull = ptr.is_null(0x1000) ? 1 : 0;
print(`${isNotNull}`); // 0

const off = ptr.offset(0x1000, 16);
print(`${off}`); // 4112

describe("fixture:ptr_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("0\n1\n0\n4112\n");
  });
});
