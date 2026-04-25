// Captura `super` em callback
import { ui, io, gc } from "rts";

class Base {
    n: number;
    constructor() { this.n = 0; }
    bump(): void { this.n = this.n + 1; }
}
class Sub extends Base {
    btn: number;
    constructor() {
        super();
        this.btn = ui.button_new(0, 0, 1, 1, "");
        ui.widget_set_callback(this.btn, () => { super.bump(); });
    }
}

const app = ui.app_new();
const win = ui.window_new(10, 10, "x");
const c = new Sub();
__class_Sub_lifted_arrow_0();
__class_Sub_lifted_arrow_0();
__class_Sub_lifted_arrow_0();
const h = gc.string_from_i64(c.n);
io.print(h);
gc.string_free(h);
