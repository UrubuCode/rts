// Generic class: Box<T>.
import { io, gc } from "rts";

class Box<T> {
  value: T;
  constructor(v: T) { this.value = v; }
  get(): T { return this.value; }
  set(v: T): void { this.value = v; }
}

const b = new Box<i64>(42);
const h = gc.string_from_i64(b.get());
io.print(h); gc.string_free(h);

b.set(99);
const h2 = gc.string_from_i64(b.get());
io.print(h2); gc.string_free(h2);
