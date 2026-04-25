// Spread literal em super(...) e super.method(...).
import { io, gc } from "rts";

class Base {
    a: number;
    b: number;
    constructor(a: number, b: number) {
        this.a = a;
        this.b = b;
    }
    addBoth(x: number, y: number): number {
        return this.a + this.b + x + y;
    }
}

class Sub extends Base {
    constructor() {
        super(...[3, 7]);
    }
    callBase(): number {
        return super.addBoth(...[100, 200]);
    }
}

const s = new Sub();
const h1 = gc.string_from_i64(s.a + s.b);
io.print(h1); gc.string_free(h1); // 10
const h2 = gc.string_from_i64(s.callBase());
io.print(h2); gc.string_free(h2); // 3+7+100+200 = 310
