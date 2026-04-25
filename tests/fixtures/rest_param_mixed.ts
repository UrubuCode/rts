// Mistura param normal + rest.
import { io, gc } from "rts";

function joinFrom(start: number, ...extra: number[]): number {
    let total = start;
    for (const n of extra) {
        total = total + n;
    }
    return total;
}

const h1 = gc.string_from_i64(joinFrom(100, 1, 2, 3));
io.print(h1); gc.string_free(h1); // 106
const h2 = gc.string_from_i64(joinFrom(50));
io.print(h2); gc.string_free(h2); // 50
