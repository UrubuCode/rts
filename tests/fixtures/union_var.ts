// Union em decl de variável: aceita reatribuição com tipos diferentes.
// O valor armazenado mantém os bits — semântica de \"any\" runtime.
import { io, gc } from "rts";

function makeNum(): number | string {
    return 42;
}

const v: number | string = makeNum();
const h = gc.string_from_i64(v as number);
io.print(h); gc.string_free(h);
