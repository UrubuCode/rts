// Initializer referenciando field anterior via this.x.
// Ordem de execução = ordem de declaração.
import { io, gc } from "rts";

class C {
    a: number = 10;
    b: number = 20;
    c: number = 999; // sera sobrescrito no ctor

    constructor() {
        this.c = this.a + this.b; // 30
    }
}

const c = new C();
const ha = gc.string_from_i64(c.a);
io.print(ha); gc.string_free(ha);

const hb = gc.string_from_i64(c.b);
io.print(hb); gc.string_free(hb);

const hc = gc.string_from_i64(c.c);
io.print(hc); gc.string_free(hc);
