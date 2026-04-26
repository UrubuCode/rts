import { describe, test, expect } from "rts:test";
import { ui, io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Captura `super` em callback

class Base {
    n: number;
    constructor() { this.n = 0; }
    bump(): void { this.n = this.n + 1; }
}
class Sub extends Base {
    btn: number;
    constructor() {
        super();
        this.btn = ui.button_new(0, 0, 1, 1, "");
        ui.widget_set_callback(this.btn, () => { super.bump(); });
    }
}

const app = ui.app_new();
const win = ui.window_new(10, 10, "x");
const c = new Sub();
__class_Sub_lifted_arrow_0(c);
__class_Sub_lifted_arrow_0(c);
__class_Sub_lifted_arrow_0(c);
const h = gc.string_from_i64(c.n);
print(h);
gc.string_free(h);

describe("fixture:closure_super", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("3\n");
  });
});
