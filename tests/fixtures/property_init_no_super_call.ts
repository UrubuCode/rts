// Sub com ctor explícito mas SEM super() chamado:
// initializers ficam no inicio (sem super pra pular).
// Limitação documentada da feature: parent não inicializa,
// mas é semântica válida quando parent é trivial.
import { io, gc } from "rts";

class Base {
    a: number = 7;
}

class Sub extends Base {
    b: number = 13;
    constructor() {
        // Não chamamos super() aqui — initializer de Sub.b roda mesmo assim.
        // a fica em estado "indefinido" (no caso, 0 porque o map está vazio).
    }
}

const s = new Sub();
const hb = gc.string_from_i64(s.b);
io.print(hb); gc.string_free(hb); // 13
