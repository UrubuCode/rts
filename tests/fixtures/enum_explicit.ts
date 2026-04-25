// Enum com valores explícitos e mistos (auto-incremento após explicit).
import { io, gc } from "rts";

enum Mask {
    Read = 1,
    Write = 2,
    Exec = 4,
    All = 7, // explicit
    Sentinel,  // 8 (All + 1)
}

const h1 = gc.string_from_i64(Mask.Read);
io.print(h1); gc.string_free(h1); // 1
const h2 = gc.string_from_i64(Mask.Write);
io.print(h2); gc.string_free(h2); // 2
const h3 = gc.string_from_i64(Mask.All);
io.print(h3); gc.string_free(h3); // 7
const h4 = gc.string_from_i64(Mask.Sentinel);
io.print(h4); gc.string_free(h4); // 8

// Bitmask: Read | Write
const rw = Mask.Read | Mask.Write;
const h5 = gc.string_from_i64(rw);
io.print(h5); gc.string_free(h5); // 3
