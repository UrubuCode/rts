// Generic constraint <T extends X>: type-erased em runtime.
import { io, gc } from "rts";

function add<T extends i64>(a: T, b: T): T {
  return a + b;
}

function max<T extends i64>(a: T, b: T): T {
  if (a > b) return a;
  return b;
}

const r1 = add<i64>(7, 8);
const h1 = gc.string_from_i64(r1);
io.print(h1); gc.string_free(h1);

const r2 = max<i64>(15, 23);
const h2 = gc.string_from_i64(r2);
io.print(h2); gc.string_free(h2);
