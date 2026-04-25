// Rest sem args passados.
import { io, gc, collections } from "rts";

function count(...nums: number[]): number {
    return collections.vec_len(nums);
}

const h1 = gc.string_from_i64(count());
io.print(h1); gc.string_free(h1); // 0
const h2 = gc.string_from_i64(count(7, 8, 9, 10, 11));
io.print(h2); gc.string_free(h2); // 5
