// Múltiplas instâncias do mesmo widget_set_callback callback.
// Antes (#148 não resolvido): última instância vencia — callback de a
// operava em b. Agora cada instância opera em si mesma.
import { ui, io, gc } from "rts";

class Counter {
    n: number;
    btn: number;
    constructor(start: number) {
        this.n = start;
        this.btn = ui.button_new(0, 0, 1, 1, "");
        ui.widget_set_callback(this.btn, () => {
            this.n = this.n + 1;
        });
    }
}

const app = ui.app_new();
const win = ui.window_new(10, 10, "x");
const a = new Counter(1000);
const b = new Counter(2000);

// Dispara callback do a (passando handle dele) e do b separados.
// Como o trampolim agora recebe `this` por parâmetro, cada instância
// tem seu callback independente.
__class_Counter_lifted_arrow_0(a);
__class_Counter_lifted_arrow_0(a);
__class_Counter_lifted_arrow_0(b);

const ha = gc.string_from_i64(a.n);
io.print(ha); gc.string_free(ha); // 1002

const hb = gc.string_from_i64(b.n);
io.print(hb); gc.string_free(hb); // 2001
