// Assertion no receiver de chamada: `(c as Counter).method()`.
// Usado quando o tipo estático foi perdido (any/unknown) e queremos
// rotear via classe específica.
import { io, gc } from "rts";

class Counter {
    n: number = 0;
    bump(): number {
        this.n = this.n + 1;
        return this.n;
    }
}

const c = new Counter();
// Roundtrip via assertion (no-op) — exercita o passthrough.
const v = (c as Counter).bump();
const h = gc.string_from_i64(v);
io.print(h); gc.string_free(h); // 1

// Cadeia: assertion + assertion.
const v2 = ((c as Counter) as Counter).bump();
const h2 = gc.string_from_i64(v2);
io.print(h2); gc.string_free(h2); // 2
