import { describe, test, expect } from "rts:test";
import { ui, io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Captura `this` em callback: read/write em campo

class Counter {
    n: number;
    btn: number;
    constructor() {
        this.n = 0;
        this.btn = ui.button_new(0, 0, 1, 1, "");
        ui.widget_set_callback(this.btn, () => {
            this.n = this.n + 7;
        });
    }
}

const app = ui.app_new();
const win = ui.window_new(10, 10, "x");
const c = new Counter();

const before = gc.string_from_i64(c.n);
print(before);
gc.string_free(before);

// O trampolim agora recebe `this` por parâmetro (path #148).
__class_Counter_lifted_arrow_0(c);
__class_Counter_lifted_arrow_0(c);
__class_Counter_lifted_arrow_0(c);

const after = gc.string_from_i64(c.n);
print(after);
gc.string_free(after);

describe("fixture:closure_this_field", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("0\n21\n");
  });
});
