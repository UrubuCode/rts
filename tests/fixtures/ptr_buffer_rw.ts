// ptr.write/read sobre buffer real (handle do namespace buffer).
import { io, gc, ptr, buffer } from "rts";

const buf = buffer.alloc_zeroed(64);
const p = buffer.ptr(buf);

// Escreve i64 no offset 0
ptr.write_i64(p, 12345);
const v1 = ptr.read_i64(p);
const h1 = gc.string_from_i64(v1);
io.print(h1); gc.string_free(h1); // 12345

// Escreve u8 no offset 8
ptr.write_u8(ptr.offset(p, 8), 0xAB);
const v2 = ptr.read_u8(ptr.offset(p, 8));
const h2 = gc.string_from_i64(v2);
io.print(h2); gc.string_free(h2); // 171

// memset 16 bytes com 0xFF a partir do offset 16
ptr.write_bytes(ptr.offset(p, 16), 0xFF, 16);
const v3 = ptr.read_u8(ptr.offset(p, 20));
const h3 = gc.string_from_i64(v3);
io.print(h3); gc.string_free(h3); // 255

buffer.free(buf);
