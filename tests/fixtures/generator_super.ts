// Generator override consumindo super.vals() via for-of.
import { io, gc } from "rts";

class Base {
  *vals() {
    yield 1;
    yield 2;
  }
}

class Derived extends Base {
  *vals() {
    for (const v of super.vals()) {
      yield v * 10;
    }
    yield 99;
  }
}

const d = new Derived();
for (const v of d.vals()) {
  const h = gc.string_from_i64(v);
  io.print(h); gc.string_free(h);
}
