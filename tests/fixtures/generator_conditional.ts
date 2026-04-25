// Generator com yield condicional (so pares).
import { io, gc } from "rts";

function* evens(n: i64) {
  for (let i = 0; i < n; i = i + 1) {
    if (i % 2 == 0) {
      yield i;
    }
  }
}

for (const v of evens(8)) {
  const h = gc.string_from_i64(v);
  io.print(h); gc.string_free(h);
}
