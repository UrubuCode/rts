// Generator com yield dentro de loop.
import { io, gc } from "rts";

function* range(start: i64, end: i64) {
  for (let i = start; i < end; i = i + 1) {
    yield i;
  }
}

for (const n of range(2, 6)) {
  const h = gc.string_from_i64(n);
  io.print(h); gc.string_free(h);
}
