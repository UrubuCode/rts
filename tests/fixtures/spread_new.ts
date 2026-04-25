// Spread literal em `new C(...args)` — desugar via expand_spread_args.
import { io, gc } from "rts";

class Pair {
    a: number;
    b: number;
    constructor(a: number, b: number) {
        this.a = a;
        this.b = b;
    }
    sum(): number { return this.a + this.b; }
}

const p = new Pair(...[7, 13]);
const h = gc.string_from_i64(p.sum());
io.print(h); gc.string_free(h); // 20
