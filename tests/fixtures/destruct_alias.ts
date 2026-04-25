// Object destructuring com alias: { x: a }.
import { io, gc } from "rts";

const obj = { width: 100, height: 50 };
const { width: w, height: h } = obj;

const h1 = gc.string_from_i64(w);
io.print(h1); gc.string_free(h1); // 100
const h2 = gc.string_from_i64(h);
io.print(h2); gc.string_free(h2); // 50
