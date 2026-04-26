// ptr.null/is_null/offset.
import { io, gc, ptr } from "rts";

const p = ptr.null();
const h1 = gc.string_from_i64(p);
io.print(h1); gc.string_free(h1); // 0

const isNull = ptr.is_null(0) ? 1 : 0;
const h2 = gc.string_from_i64(isNull);
io.print(h2); gc.string_free(h2); // 1

const isNotNull = ptr.is_null(0x1000) ? 1 : 0;
const h3 = gc.string_from_i64(isNotNull);
io.print(h3); gc.string_free(h3); // 0

const off = ptr.offset(0x1000, 16);
const h4 = gc.string_from_i64(off);
io.print(h4); gc.string_free(h4); // 4112
