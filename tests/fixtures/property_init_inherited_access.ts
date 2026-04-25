// Subclasse: initializer pode acessar field herdado do parent
import { io, gc } from "rts";

class Base {
    x: number = 5;
}

class Sub extends Base {
    y: number = 0; // sobrescrito no ctor
    constructor() {
        super();
        // x ja foi initialized pelo parent (super)
        this.y = this.x * 10; // 50
    }
}

const s = new Sub();
const hx = gc.string_from_i64(s.x);
io.print(hx); gc.string_free(hx); // 5

const hy = gc.string_from_i64(s.y);
io.print(hy); gc.string_free(hy); // 50
