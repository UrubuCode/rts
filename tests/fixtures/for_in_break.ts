// for-in respeita break/continue.
import { io } from "rts";

const obj = { a: 1, b: 2, c: 3, d: 4 };

for (const k in obj) {
    if (k == "c") { break; }
    io.print(k);
}

io.print("---");

for (const k in obj) {
    if (k == "b") { continue; }
    io.print(k);
}
