// for-in em objeto vazio: nenhum corpo executado.
import { io, collections } from "rts";

const obj = collections.map_new(); // map vazio sem inits
io.print("before");
for (const key in obj) {
    io.print("UNREACHABLE");
}
io.print("after");
