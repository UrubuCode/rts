// Array destructuring básico.
import { io, gc } from "rts";

const [a, b, c] = [10, 20, 30];

const h1 = gc.string_from_i64(a);
io.print(h1); gc.string_free(h1); // 10
const h2 = gc.string_from_i64(b);
io.print(h2); gc.string_free(h2); // 20
const h3 = gc.string_from_i64(c);
io.print(h3); gc.string_free(h3); // 30
