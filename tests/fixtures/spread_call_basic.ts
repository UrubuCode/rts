// Spread de array literal em chamada de fn user.
import { io, gc } from "rts";

function add3(a: number, b: number, c: number): number {
    return a + b + c;
}

const h = gc.string_from_i64(add3(...[1, 2, 3]));
io.print(h); gc.string_free(h); // 6
