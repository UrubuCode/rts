// Labeled break: sai do loop externo a partir do interno.
import { io, gc } from "rts";

let pairs: number = 0;
outer: for (let i = 0; i < 5; i = i + 1) {
    for (let j = 0; j < 5; j = j + 1) {
        if (i == 2 && j == 3) {
            break outer;
        }
        pairs = pairs + 1;
    }
}

const h = gc.string_from_i64(pairs);
io.print(h); gc.string_free(h); // 5+5+3 = 13
