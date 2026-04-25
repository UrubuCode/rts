// Initializer básico: ctor explícito + initializers
import { io, gc } from "rts";

class C {
    n: number = 42;
    m: number = 7;

    constructor() {
        // ctor vazio — initializers devem rodar
    }
}

const c = new C();
const h1 = gc.string_from_i64(c.n);
io.print(h1); // 42
gc.string_free(h1);

const h2 = gc.string_from_i64(c.m);
io.print(h2); // 7
gc.string_free(h2);
