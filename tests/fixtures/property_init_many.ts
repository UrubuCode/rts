// 6 campos com initializers em ordem; cada um depende do anterior.
import { io, gc } from "rts";

class C {
    a: number = 1;
    b: number = 2;
    c: number = 3;
    d: number = 4;
    e: number = 5;
    f: number = 0; // sera atualizado no ctor

    constructor() {
        this.f = this.a + this.b + this.c + this.d + this.e; // 15
    }
}

const x = new C();
const h = gc.string_from_i64(x.f);
io.print(h);
gc.string_free(h);
