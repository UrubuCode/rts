import { describe, test, expect } from "rts:test";
import { io, gc, ptr, buffer } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// ptr.copy / copy_nonoverlapping sobre buffers.

const src = buffer.alloc_zeroed(32);
const dst = buffer.alloc_zeroed(32);

// Preenche src com padrao 0x01..0x10
const psrc = buffer.ptr(src);
for (let i: i64 = 0; i < 16; i = i + 1) {
  ptr.write_u8(ptr.offset(psrc, i), i + 1);
}

// copy_nonoverlapping 16 bytes
const pdst = buffer.ptr(dst);
ptr.copy_nonoverlapping(pdst, psrc, 16);

// le primeiros 4 bytes do dst
for (let i: i64 = 0; i < 4; i = i + 1) {
  const v = ptr.read_u8(ptr.offset(pdst, i));
  const h = gc.string_from_i64(v);
  print(h); gc.string_free(h);
}

buffer.free(src);
buffer.free(dst);

describe("fixture:ptr_copy", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("1\n2\n3\n4\n");
  });
});
