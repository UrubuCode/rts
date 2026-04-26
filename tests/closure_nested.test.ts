import { describe, test, expect } from "rts:test";
import { ui, io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Arrow aninhada capturando `this`

class C {
    n: number;
    btn1: number;
    btn2: number;
    constructor() {
        this.n = 0;
        this.btn1 = ui.button_new(0, 0, 1, 1, "");
        this.btn2 = ui.button_new(0, 0, 1, 1, "");
        ui.widget_set_callback(this.btn1, () => {
            this.n = this.n + 1;
            ui.widget_set_callback(this.btn2, () => {
                this.n = this.n + 100;
            });
        });
    }
}

const app = ui.app_new();
const win = ui.window_new(10, 10, "x");
const c = new C();

// outer: incrementa em 1 e registra inner
__class_C_lifted_arrow_0(c);
const h1 = gc.string_from_i64(c.n);
print(h1);
gc.string_free(h1);

// inner: incrementa em 100
__class_C_lifted_arrow_1(c);
const h2 = gc.string_from_i64(c.n);
print(h2);
gc.string_free(h2);

describe("fixture:closure_nested", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("1\n101\n");
  });
});
