import { describe, test, expect } from "rts:test";
import { ui, io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Captura de parâmetro da fn enclosing.

function makeBumper(start: number): void {
    let total: number = 0;
    const btn = ui.button_new(0, 0, 1, 1, "");
    const cb = () => {
        total = total + start;
    };
    ui.widget_set_callback(btn, cb);
    cb();
    cb();
    cb();
    const h = gc.string_from_i64(total);
    print(h); gc.string_free(h); // 21 (start=7, 3x)
}

const app = ui.app_new();
const win = ui.window_new(10, 10, "x");
makeBumper(7);

describe("fixture:closure_local_param", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("21\n");
  });
});
