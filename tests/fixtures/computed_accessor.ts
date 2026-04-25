// Getter e setter com nome computed (literal)
import { io, gc } from "rts";

class C {
    _v: number = 0;

    get ["x"](): number {
        return this._v;
    }

    set ["x"](n: number) {
        this._v = n * 2;
    }
}

const c = new C();
c.x = 7;             // setter → _v = 14
const h = gc.string_from_i64(c.x); // getter → 14
io.print(h); gc.string_free(h);
