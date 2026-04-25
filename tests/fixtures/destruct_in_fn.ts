// Destructuring dentro de fn user.
import { io, gc } from "rts";

function process(): number {
    const arr = [7, 13, 100];
    const [first, second] = arr;
    return first + second;
}

const h = gc.string_from_i64(process());
io.print(h); gc.string_free(h); // 20
