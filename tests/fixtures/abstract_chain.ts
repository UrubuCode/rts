// Cadeia abstract → abstract → concreto
import { io, gc } from "rts";

abstract class Shape {
    abstract area(): number;
}

abstract class ColoredShape extends Shape {
    abstract describe(): number;
    // não implementa area — herda como abstract
}

class Box extends ColoredShape {
    side: number = 4;
    area(): number { return this.side * this.side; }
    describe(): number { return 100 + this.area(); }
}

const b = new Box();
const h1 = gc.string_from_i64(b.area());
io.print(h1); gc.string_free(h1); // 16

const h2 = gc.string_from_i64(b.describe());
io.print(h2); gc.string_free(h2); // 116
