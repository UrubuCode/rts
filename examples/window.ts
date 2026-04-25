import { ui, io } from "rts";

// ── App ──────────────────────────────────────────────────────────────────
const app = ui.app_new();

// ── Window ───────────────────────────────────────────────────────────────
const win = ui.window_new(500, 400, "RTS UI Demo");
ui.window_set_color(win, 30, 30, 30);

// ── Menu bar ─────────────────────────────────────────────────────────────
const menu = ui.menubar_new(0, 0, 500, 25);
ui.menubar_add(menu, "File/New",  () => { io.print("New"); });
ui.menubar_add(menu, "File/Open", () => { io.print("Open"); });
ui.menubar_add(menu, "File/Quit", () => { ui.alert("Bye!"); });
ui.menubar_add(menu, "Help/About", () => { ui.alert("RTS UI v1"); });

// ── Title label ──────────────────────────────────────────────────────────
const title = ui.frame_new(10, 35, 480, 30, "Welcome to RTS UI!");
ui.widget_set_label_color(title, 255, 200, 50);

// ── Custom-drawn canvas ───────────────────────────────────────────────────
const canvas = ui.frame_new(10, 75, 230, 150, "");
ui.widget_set_draw(canvas, () => {
    ui.set_draw_color(20, 20, 60);
    ui.draw_rect_fill(10, 75, 230, 150);
    ui.set_draw_color(100, 180, 255);
    ui.draw_circle(125, 150, 50.0);
    ui.set_draw_color(255, 80, 80);
    ui.draw_line(10, 75, 240, 225);
    ui.set_draw_color(255, 255, 255);
    ui.set_font(0, 14);
    ui.draw_text("Canvas", 95, 100);
});

// ── Input field ──────────────────────────────────────────────────────────
const inp = ui.input_new(260, 90, 220, 25, "Name:");
ui.input_set_value(inp, "World");

// ── Output field ─────────────────────────────────────────────────────────
const out = ui.output_new(260, 130, 220, 25, "Output:");

// ── Checkbox ─────────────────────────────────────────────────────────────
const chk = ui.check_new(260, 165, 150, 25, "Enable feature");
ui.check_set_value(chk, 1);

// ── Radio buttons ────────────────────────────────────────────────────────
const r1 = ui.radio_new(260, 195, 100, 20, "Option A");
const r2 = ui.radio_new(370, 195, 100, 20, "Option B");
ui.radio_set_value(r1, 1);

// ── Slider ───────────────────────────────────────────────────────────────
const slider = ui.slider_new(10, 240, 230, 20, "");
ui.slider_set_bounds(slider, 0.0, 100.0);
ui.slider_set_value(slider, 40.0);

// ── Progress bar ─────────────────────────────────────────────────────────
const prog = ui.progress_new(260, 240, 220, 20, "");
ui.progress_set_value(prog, 65.0);
ui.widget_set_color(prog, 50, 150, 50);

// ── Spinner ──────────────────────────────────────────────────────────────
const spin = ui.spinner_new(360, 165, 80, 25, "Count:");
ui.spinner_set_bounds(spin, 0.0, 99.0);
ui.spinner_set_value(spin, 5.0);

// ── TextEditor ───────────────────────────────────────────────────────────
const buf = ui.textbuf_new();
ui.textbuf_set_text(buf, "Edit me here!\nLine two.\n");
const editor = ui.texteditor_new(10, 275, 480, 80, "");
ui.texteditor_set_buffer(editor, buf);

// ── Buttons ──────────────────────────────────────────────────────────────
const btnGreet = ui.button_new(10, 365, 120, 25, "Greet");
ui.widget_set_callback(btnGreet, () => {
    const name = ui.input_value(inp);
    // Note: name is a GC handle — use gc.string_ptr/len to read
    io.print("Button clicked!");
    ui.output_set_value(out, "Hello!");
    ui.widget_set_label(out, "Output:");
});

const btnAsk = ui.button_new(140, 365, 120, 25, "Ask");
ui.widget_set_callback(btnAsk, () => {
    const yes = ui.dialog_ask("Continue?");
    if (yes) {
        io.print("User said Yes");
    } else {
        io.print("User said No");
    }
});

const btnClose = ui.button_new(370, 365, 120, 25, "Close");
ui.widget_set_color(btnClose, 160, 30, 30);

ui.widget_set_callback(btnClose, () => {
    io.print("Closing window...");
});

// ── Show & run ────────────────────────────────────────────────────────────
ui.window_end(win);
ui.window_show(win);
io.print("UI running — close window to exit");

ui.app_run(app);
io.print("done");
