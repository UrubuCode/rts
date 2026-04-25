// Spread no callsite + rest no callee — interação correta.
import { io, gc } from "rts";

function sumAll(...nums: number[]): number {
    let total = 0;
    for (const n of nums) {
        total = total + n;
    }
    return total;
}

const h = gc.string_from_i64(sumAll(...[10, 20, 30, 40]));
io.print(h); gc.string_free(h); // 100
