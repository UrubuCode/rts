import { ui, io, gc } from "rts";

class Counter {
    n: number;
    btn: number;
    out: number;

    constructor() {
        this.n = 0;
        this.out = ui.output_new(10, 10, 200, 26, "value:");
        ui.output_set_value(this.out, "0");

        this.btn = ui.button_new(10, 50, 100, 30, "+1");
        ui.widget_set_callback(this.btn, () => {
            this.n = this.n + 1;
            const h = gc.string_from_i64(this.n);
            ui.output_set_value(this.out, h);
            gc.string_free(h);
            const log = gc.string_from_i64(this.n);
            io.print(log);
            gc.string_free(log);
        });
    }
}

const app = ui.app_new();
const win = ui.window_new(240, 100, "this in callback");
const c = new Counter();
ui.window_end(win);
ui.window_show(win);
ui.app_run(app);
