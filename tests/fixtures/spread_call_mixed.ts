// Mistura args normais com spread literal.
import { io, gc } from "rts";

function sum4(a: number, b: number, c: number, d: number): number {
    return a + b + c + d;
}

const h1 = gc.string_from_i64(sum4(10, ...[1, 2], 100));
io.print(h1); gc.string_free(h1); // 113

// Múltiplos spreads.
const h2 = gc.string_from_i64(sum4(...[1, 2], ...[3, 4]));
io.print(h2); gc.string_free(h2); // 10
