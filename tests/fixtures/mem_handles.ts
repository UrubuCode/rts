// mem.drop_handle libera GC; forget_handle nao libera (vaza).
import { io, gc, mem } from "rts";

const h1 = gc.string_from_i64(42);
io.print(h1);
mem.drop_handle(h1);
io.print("dropped");

const h2 = gc.string_from_i64(99);
io.print(h2);
mem.forget_handle(h2);
io.print("forgotten");
