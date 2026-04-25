// Type assertion `as` em expressão simples.
import { io, gc } from "rts";

function getValue(): number {
    return 42;
}

const x = getValue() as number;
const h = gc.string_from_i64(x);
io.print(h); gc.string_free(h); // 42

// Forma legacy <Type>expr também aceita.
const y = (10 as number) + 5;
const h2 = gc.string_from_i64(y);
io.print(h2); gc.string_free(h2); // 15
