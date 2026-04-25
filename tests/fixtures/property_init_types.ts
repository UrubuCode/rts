// Initializers com tipos variados: string, bool, number
import { io } from "rts";

class C {
    name: string = "world";
    flag: boolean = true;
    count: number = 7;
}

const c = new C();
io.print(c.name); // world
if (c.flag) { io.print("on"); } else { io.print("off"); }
io.print(c.count == 7 ? "seven" : "not seven");
