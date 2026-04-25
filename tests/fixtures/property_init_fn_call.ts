// Initializer chamando função user-defined
import { io, gc, math } from "rts";

function compute(): number {
    return 42 + 8;
}

class C {
    a: number = compute(); // 50
    b: number = math.abs_i64(-13); // 13
}

const c = new C();
const ha = gc.string_from_i64(c.a);
io.print(ha); gc.string_free(ha);

const hb = gc.string_from_i64(c.b);
io.print(hb); gc.string_free(hb);
