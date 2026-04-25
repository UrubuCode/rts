// Template literal sem interpolação: [`name`]() ≡ name()
import { io } from "rts";

class C {
    [`hello`](): string {
        return "world";
    }
}

const c = new C();
io.print(c.hello());
