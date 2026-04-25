// catch que faz rethrow — outer catch pega.
import { io } from "rts";

try {
    try {
        throw "first";
    } catch (e) {
        io.print(`inner: ${e}`);
        throw "rethrown";
    }
} catch (e2) {
    io.print(`outer: ${e2}`);
}

io.print("end");
