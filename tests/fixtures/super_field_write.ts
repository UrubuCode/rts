// super.field = v escreve no field herdado
import { io, gc } from "rts";

class Base {
    x: number = 0;
}

class Sub extends Base {
    setBase(v: number): void {
        super.x = v;
    }
    setBaseCompound(): void {
        super.x = super.x + 100;
    }
}

const s = new Sub();
s.setBase(42);
const h1 = gc.string_from_i64(s.x);
io.print(h1); gc.string_free(h1); // 42

s.setBaseCompound(); // 42 + 100
const h2 = gc.string_from_i64(s.x);
io.print(h2); gc.string_free(h2); // 142
