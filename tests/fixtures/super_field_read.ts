// super.field para ler field herdado (sem getter)
import { io, gc } from "rts";

class Base {
    x: number = 7;
    y: number = 13;
}

class Sub extends Base {
    sumViaSuper(): number {
        return super.x + super.y; // 20
    }
    sumViaThis(): number {
        return this.x + this.y; // 20 — equivalente
    }
}

const s = new Sub();
const h1 = gc.string_from_i64(s.sumViaSuper());
io.print(h1); gc.string_free(h1);
const h2 = gc.string_from_i64(s.sumViaThis());
io.print(h2); gc.string_free(h2);
