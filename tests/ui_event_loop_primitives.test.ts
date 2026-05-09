// (#224) UI event loop primitives — app_check, app_add_timeout,
// app_repeat_timeout, app_add_idle. Smoke compile-resolve-call sem
// validar timer firing (depende de display nativo + main loop real).

import { describe, test, expect } from "rts:test";
import { ui } from "rts";

const app = ui.app_new();
const win = ui.window_new(200, 100, "test");
ui.window_show(win);

// app_check — non-blocking poll
const c1 = ui.app_check(app);
const c2 = ui.app_check(app);

// add_timeout — schedule sem fire (vai cair quando app_run rodar)
let timerScheduled = false;
const cb = () => { timerScheduled = true; };
ui.app_add_timeout(0.001, cb);
ui.app_repeat_timeout(0.001, cb);
ui.app_add_idle(cb);

// Drena alguns eventos pra confirmar que nao crasha
const w1 = ui.app_check(app);

describe("ui_event_loop_primitives", () => {
    test("app_check returns boolean", () => expect(c1).toBe(true));
    test("app_check repeat", () => expect(c2).toBe(true));
    test("scheduled callbacks don't crash", () => expect(w1).toBe(true));
});
