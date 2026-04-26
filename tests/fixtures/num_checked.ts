// num.checked_*: aritmetica que sinaliza overflow via i64::MIN.
import { io, gc, num } from "rts";

const a = num.checked_add(100, 200);
const h1 = gc.string_from_i64(a); io.print(h1); gc.string_free(h1);

const b = num.checked_div(100, 0);
const h2 = gc.string_from_i64(b); io.print(h2); gc.string_free(h2);

const c = num.checked_sub(50, 30);
const h3 = gc.string_from_i64(c); io.print(h3); gc.string_free(h3);

const d = num.checked_mul(7, 6);
const h4 = gc.string_from_i64(d); io.print(h4); gc.string_free(h4);
