// Namespace com constants exportadas.
import { io, gc } from "rts";

namespace Conf {
    export const PORT = 3000;
    export const RETRIES = 5;
}

const h1 = gc.string_from_i64(Conf.PORT);
io.print(h1); gc.string_free(h1); // 3000

const h2 = gc.string_from_i64(Conf.RETRIES);
io.print(h2); gc.string_free(h2); // 5
