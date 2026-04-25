// Object destructuring básico.
import { io, gc } from "rts";

const obj = { x: 5, y: 10 };
const { x, y } = obj;

const h1 = gc.string_from_i64(x);
io.print(h1); gc.string_free(h1); // 5
const h2 = gc.string_from_i64(y);
io.print(h2); gc.string_free(h2); // 10
