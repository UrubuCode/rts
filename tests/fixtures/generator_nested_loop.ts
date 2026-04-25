// Generator com loops aninhados.
import { io, gc } from "rts";

function* pairs(n: i64, m: i64) {
  for (let i = 0; i < n; i = i + 1) {
    for (let j = 0; j < m; j = j + 1) {
      yield i * 10 + j;
    }
  }
}

for (const v of pairs(3, 2)) {
  const h = gc.string_from_i64(v);
  io.print(h); gc.string_free(h);
}
