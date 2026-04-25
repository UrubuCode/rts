// Generator que nao yielda nada — array vazio.
import { io, gc } from "rts";

function* nope(): i64 {
  // sem yields
}

let count: i64 = 0;
for (const _v of nope()) {
  count = count + 1;
}
const h = gc.string_from_i64(count);
io.print(h); gc.string_free(h);
