// Captura de parâmetro da fn enclosing.
import { ui, io, gc } from "rts";

function makeBumper(start: number): void {
    let total: number = 0;
    const btn = ui.button_new(0, 0, 1, 1, "");
    ui.widget_set_callback(btn, () => {
        total = total + start;
    });
    __lifted_arrow_0();
    __lifted_arrow_0();
    __lifted_arrow_0();
    const h = gc.string_from_i64(total);
    io.print(h); gc.string_free(h); // 21 (start=7, 3x)
}

const app = ui.app_new();
const win = ui.window_new(10, 10, "x");
makeBumper(7);
