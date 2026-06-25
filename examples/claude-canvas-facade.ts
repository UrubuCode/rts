import egui from "rts:egui";

// rts:canvas — UI IMEDIATA ergonômica, SEM DOM. `Canvas`/`createCanvas` vêm do
// prelude (sobre render.*/input.* abstratos). O egui é só o backend que pinta;
// trocar de backend não muda este código. Mostra que a abstração serve fora do
// DOM — um "modo canvas" direto.
//   target/release/rts.exe run examples/claude-canvas-facade.ts

const win = egui.openWindow("rts:canvas (UI imediata, sem DOM)", 460, 320, 0);
egui.moveWindow(win, 2200, 400); // tela 2

const cv = createCanvas(win); // fachada ergonômica

let counter = 0;
let bg = 0x12161CFF;

while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  cv.begin();

  cv.fillRect(0, 0, 460, 320, bg);
  cv.text(20, 16, "Canvas ergonomico sobre render.* (sem DOM)", 0xB0B8C0FF, 15);
  cv.text(20, 50, "Contador: " + counter, 0xFFFFFFFF, 22);

  // botão de conveniência: desenha + hit-test + clique embutidos
  if (cv.button(20, 100, 180, 48, "Incrementar")) {
    counter = counter + 1;
  }
  if (cv.button(220, 100, 180, 48, "Trocar fundo")) {
    bg = bg === 0x12161CFF ? 0x1A1228FF : 0x12161CFF;
  }

  // texto centralizado usando measure
  const msg = "clique nos botoes";
  const w = cv.measure(msg, 14);
  cv.text((460 - w) / 2, 180, msg, 0x99CCFFFF, 14);

  cv.line(20, 220, 440, 220, 2, 0x3399FFFF);
  cv.box(20, 240, 420, 56, 0x1E2A3AFF, 2, 0x33CC88FF, 10);
  cv.text(36, 258, "Tudo via Canvas: egui e so o backend plugavel.", 0xC0C8D0FF, 14);

  cv.end();
}

egui.close(win);
