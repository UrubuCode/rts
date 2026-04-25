// Computed property name (field): `["x"]: number = 42`
import { io, gc } from "rts";

class C {
    ["count"]: number = 0;

    ["bump"](): void {
        this.count = this.count + 1;
    }
}

const c = new C();
c.bump();
c.bump();
c.bump();

const h = gc.string_from_i64(c.count);
io.print(h); gc.string_free(h); // 3
