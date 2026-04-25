// `yield*` delega para outro iteravel.
import { io, gc } from "rts";

function* inner() {
  yield 1;
  yield 2;
}

function* outer() {
  yield 0;
  yield* inner();
  yield 3;
}

for (const v of outer()) {
  const h = gc.string_from_i64(v);
  io.print(h); gc.string_free(h);
}
