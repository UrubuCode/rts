// `as const`, non-null `!`, satisfies (todos no-op no codegen).
import { io, gc } from "rts";

function maybe(): number {
    return 7;
}

const v = maybe()!;          // non-null: passthrough
const c = (3 + 4) as const;  // as const: passthrough

const h1 = gc.string_from_i64(v);
io.print(h1); gc.string_free(h1); // 7
const h2 = gc.string_from_i64(c);
io.print(h2); gc.string_free(h2); // 7
