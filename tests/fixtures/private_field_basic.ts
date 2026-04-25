// Private field acessível só dentro da classe
import { io, gc } from "rts";

class Counter {
    #count: number = 0;

    inc(): void {
        this.#count = this.#count + 1;
    }

    value(): number {
        return this.#count;
    }
}

const c = new Counter();
const h0 = gc.string_from_i64(c.value()); io.print(h0); gc.string_free(h0); // 0

c.inc();
c.inc();
c.inc();

const h3 = gc.string_from_i64(c.value()); io.print(h3); gc.string_free(h3); // 3
