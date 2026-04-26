// mem.size_of_* / align_of_*: layout primitives.
import { io, gc, mem } from "rts";

const h1 = gc.string_from_i64(mem.size_of_i64);
io.print(h1); gc.string_free(h1); // 8
const h2 = gc.string_from_i64(mem.size_of_f64);
io.print(h2); gc.string_free(h2); // 8
const h3 = gc.string_from_i64(mem.size_of_i32);
io.print(h3); gc.string_free(h3); // 4
const h4 = gc.string_from_i64(mem.size_of_bool);
io.print(h4); gc.string_free(h4); // 1
const h5 = gc.string_from_i64(mem.align_of_i64);
io.print(h5); gc.string_free(h5); // 8
