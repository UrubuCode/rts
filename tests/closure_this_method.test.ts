import { describe, test, expect } from "rts:test";
import { ui, io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Captura `this` em callback: chama this.method()

class C {
    n: number;
    btn: number;
    constructor() {
        this.n = 0;
        this.btn = ui.button_new(0, 0, 1, 1, "");
        ui.widget_set_callback(this.btn, () => { this.bump(); });
    }
    bump(): void {
        this.n = this.n + 5;
    }
}

const app = ui.app_new();
const win = ui.window_new(10, 10, "x");
const c = new C();
__class_C_lifted_arrow_0(c);
__class_C_lifted_arrow_0(c);
const h = gc.string_from_i64(c.n);
print(h);
gc.string_free(h);

describe("fixture:closure_this_method", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("10\n");
  });
});
