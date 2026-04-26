import { describe, test, expect } from "rts:test";
import { ui, io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Captura de local em arrow callback dentro de função user.

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
    print(h); gc.string_free(h); // 3
}

const app = ui.app_new();
const win = ui.window_new(10, 10, "x");
setup();

describe("fixture:closure_local_capture", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("3\n");
  });
});
