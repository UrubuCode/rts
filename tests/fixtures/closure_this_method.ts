// Captura `this` em callback: chama this.method()
import { ui, io, gc } from "rts";

class C {
    n: number;
    btn: number;
    constructor() {
        this.n = 0;
        this.btn = ui.button_new(0, 0, 1, 1, "");
        ui.widget_set_callback(this.btn, () => { this.bump(); });
    }
    bump(): void {
        this.n = this.n + 5;
    }
}

const app = ui.app_new();
const win = ui.window_new(10, 10, "x");
const c = new C();
__class_C_lifted_arrow_0(c);
__class_C_lifted_arrow_0(c);
const h = gc.string_from_i64(c.n);
io.print(h);
gc.string_free(h);
