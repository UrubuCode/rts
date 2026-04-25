// Namespace contendo enum.
import { io, gc } from "rts";

namespace Net {
    export enum Status {
        Ok,
        NotFound = 404,
        ServerError = 500,
    }
}

const h1 = gc.string_from_i64(Net.Status.Ok);
io.print(h1); gc.string_free(h1); // 0
const h2 = gc.string_from_i64(Net.Status.NotFound);
io.print(h2); gc.string_free(h2); // 404
const h3 = gc.string_from_i64(Net.Status.ServerError);
io.print(h3); gc.string_free(h3); // 500
