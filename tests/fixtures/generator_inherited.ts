// Generator herdado e proprio em subclasse.
import { io, gc } from "rts";

class Base {
  *vals() {
    yield 1;
    yield 2;
  }
}

class Derived extends Base {
  *more() {
    yield 10;
    yield 20;
  }
}

const d = new Derived();
io.print("base:");
for (const v of d.vals()) {
  const h = gc.string_from_i64(v);
  io.print(h); gc.string_free(h);
}
io.print("derived:");
for (const v of d.more()) {
  const h = gc.string_from_i64(v);
  io.print(h); gc.string_free(h);
}
