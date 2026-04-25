// Generic function: identity<T>.
import { io, gc } from "rts";

function identity<T>(x: T): T {
  return x;
}

const a = identity<i64>(42);
const h = gc.string_from_i64(a);
io.print(h); gc.string_free(h);

const b = identity<i64>(-7);
const h2 = gc.string_from_i64(b);
io.print(h2); gc.string_free(h2);
