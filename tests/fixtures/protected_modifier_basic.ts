// `protected` permite acesso em subclasses.
import { io, gc } from "rts";

class Base {
    protected x: number = 10;
}

class Sub extends Base {
    triple(): number {
        return this.x * 3; // OK: Sub estende Base
    }
}

const s = new Sub();
const h = gc.string_from_i64(s.triple());
io.print(h); gc.string_free(h); // 30
