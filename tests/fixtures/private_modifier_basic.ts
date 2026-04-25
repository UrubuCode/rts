// `private` keyword: acesso permitido só dentro do corpo da classe.
import { io, gc } from "rts";

class C {
    private n: number;
    constructor() {
        this.n = 0;
    }
    bump(): void {
        this.n = this.n + 5;
    }
    value(): number {
        return this.n;
    }
}

const c = new C();
c.bump();
c.bump();
const h = gc.string_from_i64(c.value());
io.print(h); gc.string_free(h); // 10
