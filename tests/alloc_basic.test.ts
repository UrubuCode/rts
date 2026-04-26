import { describe, test, expect } from "rts:test";
import { io, gc, alloc, ptr } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// alloc / dealloc + write/read via ptr.

const p = alloc.alloc_zeroed(64, 8);
if (p == 0) {
  print("FAIL: alloc retornou 0");
} else {
  print("alloc-ok");
}

ptr.write_i64(p, 7777);
const v = ptr.read_i64(p);
const h = gc.string_from_i64(v);
print(h); gc.string_free(h); // 7777

alloc.dealloc(p, 64, 8);
print("dealloc-ok");

describe("fixture:alloc_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("alloc-ok\n7777\ndealloc-ok\n");
  });
});
