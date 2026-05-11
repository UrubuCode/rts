import { ui, io } from "rts";

let display: string = "0";
let label: number = 0;

function refresh(): void {
    ui.output_set_value(label, display);
}

function pushDigit(d: string): void {
    if (display === "0") {
        display = d;
    } else {
        display = display + d;
    }
    io.print("pushed " + d + ", display=" + display);
    refresh();
}

function on7(): void { pushDigit("7"); }
function on8(): void { pushDigit("8"); }

const app = ui.app_new();
const win = ui.window_new(240, 160, "smoke2");
label = ui.output_new(10, 10, 220, 40, "");
ui.output_set_value(label, "0");
const b7 = ui.button_new(10, 60, 100, 80, "7");
ui.widget_set_callback(b7, on7);
const b8 = ui.button_new(120, 60, 100, 80, "8");
ui.widget_set_callback(b8, on8);
ui.window_end(win);
ui.window_show(win);
io.print("ready");
ui.app_run(app);
