import { describe, test, expect } from "rts:test";
import { gc, ffi, buffer, ptr } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 1) cstring_new / cstring_ptr / cstring_free roundtrip
const cs = ffi.cstring_new("hello");
if (cs == 0) {
  print("FAIL: cstring_new retornou 0");
} else {
  print("cstring-ok");
}

const p = ffi.cstring_ptr(cs);
if (p == 0) {
  print("FAIL: cstring_ptr 0");
} else {
  print("ptr-ok");
}

// cstr_len lendo o ponteiro da CString recem-criada
const len = ffi.cstr_len(p);
const hLen = gc.string_from_i64(len);
print(hLen); gc.string_free(hLen); // 5

// cstr_from_ptr re-le como string handle
const back = ffi.cstr_from_ptr(p);
print(back); gc.string_free(back); // hello

// cstr_to_str (UTF-8 estrito)
const back2 = ffi.cstr_to_str(p);
print(back2); gc.string_free(back2); // hello

ffi.cstring_free(cs);

// 2) cstr_from_ptr lendo de um buffer construido manualmente
const buf = buffer.alloc_zeroed(8);
const bp = buffer.ptr(buf);
ptr.write_u8(ptr.offset(bp, 0), 0x41); // 'A'
ptr.write_u8(ptr.offset(bp, 1), 0x42); // 'B'
ptr.write_u8(ptr.offset(bp, 2), 0x43); // 'C'
ptr.write_u8(ptr.offset(bp, 3), 0x00); // \0
const fromBuf = ffi.cstr_from_ptr(bp);
print(fromBuf); gc.string_free(fromBuf); // ABC
const lenBuf = ffi.cstr_len(bp);
const hLenBuf = gc.string_from_i64(lenBuf);
print(hLenBuf); gc.string_free(hLenBuf); // 3
buffer.free(buf);

// 3) osstr_from_str roundtrip
const os = ffi.osstr_from_str("world");
if (os == 0) {
  print("FAIL: osstr_from_str retornou 0");
} else {
  print("osstr-ok");
}
const osBack = ffi.osstr_to_str(os);
print(osBack); gc.string_free(osBack); // world
ffi.osstr_free(os);

describe("fixture:ffi_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "cstring-ok\nptr-ok\n5\nhello\nhello\nABC\n3\nosstr-ok\nworld\n"
    );
  });
});
