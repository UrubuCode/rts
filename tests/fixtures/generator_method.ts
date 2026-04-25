// Generator como metodo de classe acessando this.
import { io, gc } from "rts";

class Counter {
  base: i64 = 100;
  *bumps() {
    yield this.base + 1;
    yield this.base + 2;
    yield this.base + 3;
  }
}

const c = new Counter();
for (const v of c.bumps()) {
  const h = gc.string_from_i64(v);
  io.print(h); gc.string_free(h);
}
