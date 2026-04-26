import { describe, test, expect } from "rts:test";
import { io, gc, alloc, ptr } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// alloc + realloc preservando dados.

const p = alloc.alloc_zeroed(16, 8);
ptr.write_i64(p, 12345);
ptr.write_i64(ptr.offset(p, 8), 67890);

// Realoca para 64 bytes — dados preservados.
const p2 = alloc.realloc(p, 16, 8, 64);
if (p2 == 0) {
  print("FAIL: realloc retornou 0");
} else {
  const v1 = ptr.read_i64(p2);
  const v2 = ptr.read_i64(ptr.offset(p2, 8));
  const h1 = gc.string_from_i64(v1); print(h1); gc.string_free(h1); // 12345
  const h2 = gc.string_from_i64(v2); print(h2); gc.string_free(h2); // 67890
  alloc.dealloc(p2, 64, 8);
}

describe("fixture:alloc_realloc", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("12345\n67890\n");
  });
});
