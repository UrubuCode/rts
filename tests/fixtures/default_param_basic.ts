// Default parameter: chamada sem o arg usa o default.
import { io, gc } from "rts";

function add(a: number, b: number = 10): number {
    return a + b;
}

const h1 = gc.string_from_i64(add(5));
io.print(h1); gc.string_free(h1); // 15 (5 + default 10)

const h2 = gc.string_from_i64(add(5, 100));
io.print(h2); gc.string_free(h2); // 105
