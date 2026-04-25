// Generator simples com yield literal.
import { io, gc } from "rts";

function* gen() {
  yield 1;
  yield 2;
  yield 3;
}

for (const n of gen()) {
  const h = gc.string_from_i64(n);
  io.print(h); gc.string_free(h);
}
