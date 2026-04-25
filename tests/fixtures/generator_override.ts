// Override de generator com virtual dispatch.
import { io, gc } from "rts";

class Base {
  *vals() {
    yield 1;
    yield 2;
  }
}

class Derived extends Base {
  *vals() {
    yield 100;
    yield 200;
  }
}

const d: Base = new Derived();
for (const v of d.vals()) {
  const h = gc.string_from_i64(v);
  io.print(h); gc.string_free(h);
}
