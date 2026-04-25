// Ctor pode sobrescrever initializer (initializer roda primeiro)
import { io, gc } from "rts";

class C {
    n: number = 1;

    constructor(arg: number) {
        // initializer rodou: this.n = 1 antes desta linha
        this.n = arg;  // sobrescreve
    }
}

const c = new C(99);
const h = gc.string_from_i64(c.n);
io.print(h); // 99
gc.string_free(h);
