// Método async em classe.
import { io, gc } from "rts";

class Service {
    base: number = 100;

    async getData(): Promise<number> {
        return this.base;
    }

    async total(): Promise<number> {
        const d = await this.getData();
        return d + 7;
    }
}

const s = new Service();
const r = s.total();
const h = gc.string_from_i64(r as number);
io.print(h); gc.string_free(h); // 107
