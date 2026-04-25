// Initializer com expressão composta (não só literal)
import { io, gc, math } from "rts";

class C {
    a: number = 2 + 3;
    b: number = math.abs_i64(-7);
    s: string = "hello" + " world";
}

const c = new C();
const ha = gc.string_from_i64(c.a);
io.print(ha); // 5
gc.string_free(ha);

const hb = gc.string_from_i64(c.b);
io.print(hb); // 7
gc.string_free(hb);

io.print(c.s); // hello world
