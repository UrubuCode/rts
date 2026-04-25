// readonly: pode ser atribuído no constructor
import { io, gc } from "rts";

class Point {
    readonly x: number;
    readonly y: number;
    constructor(x: number, y: number) {
        this.x = x; // OK no ctor
        this.y = y;
    }
    sum(): number { return this.x + this.y; }
}

const p = new Point(7, 13);
const h = gc.string_from_i64(p.sum());
io.print(h); gc.string_free(h); // 20
