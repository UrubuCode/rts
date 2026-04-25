// Union em parâmetro: aceita ambos os tipos.
import { io } from "rts";

function describe(x: string | number): string {
    return "got";
}

io.print(describe(42));
io.print(describe("hello"));
io.print(describe(0));
