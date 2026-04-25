// `expr as ClassName` permite chamar métodos quando o tipo dinâmico
// não é conhecido estaticamente.
import { io, gc } from "rts";

class Counter {
    n: number = 0;
    bump(): number {
        this.n = this.n + 1;
        return this.n;
    }
}

function makeAny(): number {
    // Retorna um number mas que na verdade é handle de Counter.
    const c = new Counter();
    return c as number; // unsafe cast — handle vira number
}

const handle = makeAny();
const c = handle as Counter; // recupera tipo
const h1 = gc.string_from_i64(c.bump());
io.print(h1); gc.string_free(h1); // 1

const h2 = gc.string_from_i64(c.bump());
io.print(h2); gc.string_free(h2); // 2
