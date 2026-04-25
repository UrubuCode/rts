// catch sem binding (ES2019): `catch { ... }` em vez de `catch (e) { ... }`.
import { io } from "rts";

try {
    io.print("try");
    throw "err";
} catch {
    io.print("caught");
}

io.print("end");
