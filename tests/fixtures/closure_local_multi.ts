// Múltiplas locais capturadas no mesmo callback.
import { ui, io, gc } from "rts";

function setup(): void {
    let a: number = 100;
    let b: number = 200;
    const btn = ui.button_new(0, 0, 1, 1, "");
    ui.widget_set_callback(btn, () => {
        a = a + 1;
        b = b + 10;
    });
    __lifted_arrow_0();
    __lifted_arrow_0();
    const ha = gc.string_from_i64(a);
    io.print(ha); gc.string_free(ha); // 102
    const hb = gc.string_from_i64(b);
    io.print(hb); gc.string_free(hb); // 220
}

const app = ui.app_new();
const win = ui.window_new(10, 10, "x");
setup();
