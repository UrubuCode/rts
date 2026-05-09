import { describe, test, expect } from "rts:test";
import { ui } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#383) Antes: usava `import { app_new, ... } from "rts:ui"` (sintaxe
// que so' funcionava por warning + segfault em runtime). Apos #383,
// idents nao-resolvidos sao hard-fail em compile time, entao migrado
// para `import { ui } from "rts"` + dispatch via namespace.
const app = ui.app_new();
print("app created");

const win = ui.window_new(400, 300, "Hello RTS");
print("window created");

const lbl = ui.frame_new(10, 10, 380, 40, "Welcome to RTS UI!");
print("frame created");

const btn = ui.button_new(150, 200, 100, 40, "Click me");
print("button created");

ui.window_end(win);
ui.window_show(win);
print("window shown");

ui.app_run(app);
print("done");

describe("fixture:ui_window", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("app created\nwindow created\nframe created\nbutton created\nwindow shown\ndone\n");
  });
});
