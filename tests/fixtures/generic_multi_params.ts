// Multi type params <K, V>.
import { io, gc } from "rts";

function makePair<K, V>(k: K, v: V): K {
  return k;
}

function takeSecond<K, V>(k: K, v: V): V {
  return v;
}

const a = makePair<i64, i64>(99, 200);
const b = takeSecond<i64, i64>(99, 200);

const h1 = gc.string_from_i64(a);
io.print(h1); gc.string_free(h1);
const h2 = gc.string_from_i64(b);
io.print(h2); gc.string_free(h2);
