// Default que é expressão composta + chamada.
import { io, gc, math } from "rts";

function compute(x: number = math.abs_i64(-5)): number {
    return x * 2;
}

const h1 = gc.string_from_i64(compute());      // 5 * 2 = 10
io.print(h1); gc.string_free(h1);

const h2 = gc.string_from_i64(compute(7));     // 14
io.print(h2); gc.string_free(h2);
