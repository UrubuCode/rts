// wrapping_*: aritmetica modular.
import { io, gc, num } from "rts";

// MAX + 1 -> MIN (modular wrap).
const a = num.wrapping_add(9223372036854775807, 1);
const h1 = gc.string_from_i64(a); io.print(h1); gc.string_free(h1);

// 0 - 1 -> -1
const b = num.wrapping_sub(0, 1);
const h2 = gc.string_from_i64(b); io.print(h2); gc.string_free(h2);

const c = num.wrapping_neg(42);
const h3 = gc.string_from_i64(c); io.print(h3); gc.string_free(h3);

const d = num.wrapping_shl(1, 4);
const h4 = gc.string_from_i64(d); io.print(h4); gc.string_free(h4);

const e = num.wrapping_shr(256, 4);
const h5 = gc.string_from_i64(e); io.print(h5); gc.string_free(h5);
