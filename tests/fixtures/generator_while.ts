// Generator com while loop.
import { io, gc } from "rts";

function* fib_until(max: i64) {
  let a: i64 = 0;
  let b: i64 = 1;
  while (a < max) {
    yield a;
    const t = a + b;
    a = b;
    b = t;
  }
}

for (const n of fib_until(20)) {
  const h = gc.string_from_i64(n);
  io.print(h); gc.string_free(h);
}
