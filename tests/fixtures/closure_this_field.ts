// Captura `this` em callback: read/write em campo
import { ui, io, gc } from "rts";

class Counter {
    n: number;
    btn: number;
    constructor() {
        this.n = 0;
        this.btn = ui.button_new(0, 0, 1, 1, "");
        ui.widget_set_callback(this.btn, () => {
            this.n = this.n + 7;
        });
    }
}

const app = ui.app_new();
const win = ui.window_new(10, 10, "x");
const c = new Counter();

const before = gc.string_from_i64(c.n);
io.print(before);
gc.string_free(before);

__class_Counter_lifted_arrow_0();
__class_Counter_lifted_arrow_0();
__class_Counter_lifted_arrow_0();

const after = gc.string_from_i64(c.n);
io.print(after);
gc.string_free(after);
