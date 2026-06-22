import { describe, test, expect } from "rts:test";
import { io, alloc, ptr } from "rts";

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
  print(`${v1}`); // 12345
  print(`${v2}`); // 67890
  alloc.dealloc(p2, 64, 8);
}

describe("fixture:alloc_realloc", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("12345\n67890\n");
  });
});
