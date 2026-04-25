// Private #x e public x coexistem (mangling separado)
import { io, gc } from "rts";

class C {
    x: number = 100;
    #x: number = 999;

    bothValues(): number {
        return this.x + this.#x; // 1099
    }
}

const c = new C();
const h = gc.string_from_i64(c.bothValues());
io.print(h); gc.string_free(h);

const hx = gc.string_from_i64(c.x);
io.print(hx); gc.string_free(hx); // 100 (público)
