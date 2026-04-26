// saturating_*: clamp em i64::MIN/MAX.
import { io, gc, num } from "rts";

// 9_000_000_000_000_000_000 + 9_000_000_000_000_000_000 saturaria.
const a = num.saturating_add(9000000000000000000, 9000000000000000000);
const h1 = gc.string_from_i64(a); io.print(h1); gc.string_free(h1);

const b = num.saturating_sub(-9000000000000000000, 9000000000000000000);
const h2 = gc.string_from_i64(b); io.print(h2); gc.string_free(h2);

const c = num.saturating_mul(3, 7);
const h3 = gc.string_from_i64(c); io.print(h3); gc.string_free(h3);
