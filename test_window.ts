import { io, window, process } from "rts";

const win = window.create("RTS Canvas", 800, 600);
const h: u64 = io.unwrap_or(win, 0);

let frame: i32 = 0;
let x: i32 = 50;
let dir: i32 = 3;

while (window.is_open(h)) {
    window.clear(h, 240, 240, 245);

    x = x + dir;
    if (x > 550) { dir = 0 - 3; }
    if (x < 50) { dir = 3; }

    window.fill_rect(h, x, 200, 200, 150, 220, 30, 30);
    window.fill_rect(h, 400, 80 + (frame % 350), 150, 150, 30, 180, 30);
    window.fill_rect(h, 100, 430, 600, 50, 30, 30, 200);

    window.present(h);
    window.poll_event(h);
    process.sleep(16);
    frame = frame + 1;
}

io.print("closed after " + frame + " frames");
