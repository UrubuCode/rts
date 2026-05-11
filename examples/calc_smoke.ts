import { ui, io } from "rts";

let counter: number = 0;
let label: number = 0;

function tick(): void {
    counter = counter + 1;
    io.print("tick fired, counter=" + counter.toString());
    ui.output_set_value(label, "count=" + counter.toString());
}

const app = ui.app_new();
const win = ui.window_new(220, 120, "smoke");
label = ui.output_new(10, 10, 200, 30, "");
ui.output_set_value(label, "count=0");
const btn = ui.button_new(10, 50, 200, 50, "Click me");
ui.widget_set_callback(btn, tick);
ui.window_end(win);
ui.window_show(win);
io.print("ready");
ui.app_run(app);
