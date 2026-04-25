// Sem ctor explícito: initializers ainda rodam
import { io, gc } from "rts";

class C {
    n: number = 100;
    m: number = 200;
}

const c = new C();
const h1 = gc.string_from_i64(c.n);
io.print(h1); // 100
gc.string_free(h1);

const h2 = gc.string_from_i64(c.m);
io.print(h2); // 200
gc.string_free(h2);
