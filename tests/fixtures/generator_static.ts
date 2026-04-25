// Generator como metodo static.
import { io, gc } from "rts";

class Util {
  static *range(n: i64) {
    for (let i = 0; i < n; i = i + 1) {
      yield i;
    }
  }
}

for (const v of Util.range(4)) {
  const h = gc.string_from_i64(v);
  io.print(h); gc.string_free(h);
}
