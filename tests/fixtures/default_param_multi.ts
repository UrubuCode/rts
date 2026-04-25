// Múltiplos defaults; chamadas com 1, 2 ou 3 args.
import { io, gc } from "rts";

function combine(a: number, b: number = 10, c: number = 100): number {
    return a + b + c;
}

const h1 = gc.string_from_i64(combine(1));         // 1+10+100 = 111
io.print(h1); gc.string_free(h1);

const h2 = gc.string_from_i64(combine(1, 2));      // 1+2+100 = 103
io.print(h2); gc.string_free(h2);

const h3 = gc.string_from_i64(combine(1, 2, 3));   // 6
io.print(h3); gc.string_free(h3);
