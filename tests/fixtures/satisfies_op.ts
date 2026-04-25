// `expr satisfies T` — passthrough no codegen (igual `as`).
// Útil pra TS validar tipo sem alterar o tipo inferido do expr.
import { io, gc } from "rts";

const x = 42 satisfies number;
const h = gc.string_from_i64(x);
io.print(h); gc.string_free(h); // 42

function compute(): number {
    return (10 + 5) satisfies number;
}

const h2 = gc.string_from_i64(compute());
io.print(h2); gc.string_free(h2); // 15
