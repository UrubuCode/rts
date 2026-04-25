// Rest param: empacota args num array.
import { io, gc } from "rts";

function sum(...nums: number[]): number {
    let total = 0;
    for (const n of nums) {
        total = total + n;
    }
    return total;
}

const h = gc.string_from_i64(sum(1, 2, 3, 4));
io.print(h); gc.string_free(h); // 10
