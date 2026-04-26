import { describe, test, expect } from "rts:test";
import { io, gc, ptr, buffer } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// ptr.write/read sobre buffer real (handle do namespace buffer).

const buf = buffer.alloc_zeroed(64);
const p = buffer.ptr(buf);

// Escreve i64 no offset 0
ptr.write_i64(p, 12345);
const v1 = ptr.read_i64(p);
const h1 = gc.string_from_i64(v1);
print(h1); gc.string_free(h1); // 12345

// Escreve u8 no offset 8
ptr.write_u8(ptr.offset(p, 8), 0xAB);
const v2 = ptr.read_u8(ptr.offset(p, 8));
const h2 = gc.string_from_i64(v2);
print(h2); gc.string_free(h2); // 171

// memset 16 bytes com 0xFF a partir do offset 16
ptr.write_bytes(ptr.offset(p, 16), 0xFF, 16);
const v3 = ptr.read_u8(ptr.offset(p, 20));
const h3 = gc.string_from_i64(v3);
print(h3); gc.string_free(h3); // 255

buffer.free(buf);

describe("fixture:ptr_buffer_rw", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("12345\n171\n255\n");
  });
});
