// Captura de local em arrow callback dentro de função user.
import { ui, io, gc } from "rts";

function setup(): void {
    let count: number = 0;
    const btn = ui.button_new(0, 0, 1, 1, "");
    ui.widget_set_callback(btn, () => {
        count = count + 1;
    });
    // Dispara o callback diretamente pra simular cliques.
    __lifted_arrow_0();
    __lifted_arrow_0();
    __lifted_arrow_0();
    const h = gc.string_from_i64(count);
    io.print(h); gc.string_free(h); // 3
}

const app = ui.app_new();
const win = ui.window_new(10, 10, "x");
setup();
