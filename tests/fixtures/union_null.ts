// Union com null: aceita null como sentinel (handle 0).
import { io, gc } from "rts";

function maybe(x: number): number | null {
    if (x > 0) { return x; }
    return null;
}

const a = maybe(5);
const ha = gc.string_from_i64(a as number);
io.print(ha); gc.string_free(ha); // 5

const b = maybe(-1);
// b é null ≡ 0 em RTS
const hb = gc.string_from_i64(b as number);
io.print(hb); gc.string_free(hb); // 0
