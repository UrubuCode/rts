import { app_new, app_run, window_new, window_end, window_show, button_new, frame_new, widget_set_label } from "rts:ui";
import { print } from "rts:io";

const app = app_new();
print("app created");

const win = window_new(400, 300, "Hello RTS");
print("window created");

const lbl = frame_new(10, 10, 380, 40, "Welcome to RTS UI!");
print("frame created");

const btn = button_new(150, 200, 100, 40, "Click me");
print("button created");

window_end(win);
window_show(win);
print("window shown");

app_run(app);
print("done");
