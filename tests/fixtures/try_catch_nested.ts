// try/catch aninhado: catch interno absorve, catch externo continua sem erro.
import { io } from "rts";

try {
    try {
        throw "inner";
    } catch (e) {
        io.print(`inner caught: ${e}`);
    }
    io.print("after inner");
} catch (e2) {
    io.print(`outer would catch: ${e2}`);  // não deve disparar
}

io.print("end");
