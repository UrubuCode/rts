// Numeric enum básico, auto-incremento de 0.
import { io, gc } from "rts";

enum Status {
    Pending,
    Active,
    Closed,
}

const h0 = gc.string_from_i64(Status.Pending);
io.print(h0); gc.string_free(h0); // 0
const h1 = gc.string_from_i64(Status.Active);
io.print(h1); gc.string_free(h1); // 1
const h2 = gc.string_from_i64(Status.Closed);
io.print(h2); gc.string_free(h2); // 2
