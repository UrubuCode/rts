// Cada instância recebe sua própria cópia dos initializers
import { io, gc } from "rts";

class C {
    n: number = 100;
}

const a = new C();
const b = new C();
const k = new C();

a.n = 1;
b.n = 2;
// k.n permanece 100 (initializer)

const ha = gc.string_from_i64(a.n);
const hb = gc.string_from_i64(b.n);
const hk = gc.string_from_i64(k.n);
io.print(ha); gc.string_free(ha); // 1
io.print(hb); gc.string_free(hb); // 2
io.print(hk); gc.string_free(hk); // 100
