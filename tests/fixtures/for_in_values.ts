// Itera keys e usa map_get pra ler valores.
import { io, gc, collections } from "rts";

const obj = { x: 10, y: 20, z: 30 };

for (const key in obj) {
    // collections.map_get aceita string handle direto via codegen Handle→StrPtr
    const val = collections.map_get(obj, key);
    const h = gc.string_from_i64(val);
    io.print(key + "=" + h);
    gc.string_free(h);
}
