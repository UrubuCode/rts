import egui from "rts:egui";
import input from "rts:input";

// Mouse rico (input fase 2): drag (arrastar um quadrado), double-click, press/
// release, delta, e cursor (mãozinha sobre o quadrado). Tudo via input.* — o egui
// é só o backend que captura (trocavel). Validar arrastando o quadrado laranja.
//   target/release/rts.exe run examples/claude-mouse.ts

const app = createAppAt("Mouse rico (drag/double/cursor)", 500, 380, 2150, 420);

// quadrado arrastavel
let qx = 200;
let qy = 150;
const qs = 80;
let grabbing = 0;   // arrastando este quadrado?
let offx = 0;       // offset do grab
let offy = 0;
let doubles = 0;
let releases = 0;

while (app.running()) {
  if (!app.beginFrame()) break;
  app.fillRect(0, 0, 500, 380, 0x12161CFF);
  app.text(16, 12, "Arraste o quadrado. Double-click conta. Cursor muda.", 0xB0B8C0FF, 14);

  const mx = input.mouseX(app._win);
  const my = input.mouseY(app._win);
  const over = mx >= qx && mx <= qx + qs && my >= qy && my <= qy + qs;

  // CURSOR: mãozinha (grab) sobre o quadrado, grabbing enquanto arrasta
  if (grabbing !== 0) {
    input.setCursor(app._win, 4); // grabbing
  } else if (over) {
    input.setCursor(app._win, 3); // grab
  }

  // PRESS inicia o drag (guarda o offset); RELEASE solta
  if (over && input.mousePressed(app._win, 0) !== 0) {
    grabbing = 1;
    offx = mx - qx;
    offy = my - qy;
  }
  if (input.mouseReleased(app._win, 0) !== 0) {
    if (grabbing !== 0) releases = releases + 1;
    grabbing = 0;
  }
  // DRAG: enquanto arrasta, segue o mouse (menos o offset)
  if (grabbing !== 0) {
    qx = mx - offx;
    qy = my - offy;
  }

  // DOUBLE-CLICK sobre o quadrado
  if (over && input.mouseDoubleClicked(app._win, 0) !== 0) {
    doubles = doubles + 1;
  }

  // desenha o quadrado (cor muda com hover/grab)
  let color = 0xFF8800FF;
  if (grabbing !== 0) color = 0xFFAA33FF;
  else if (over) color = 0xFF9911FF;
  app.box(qx, qy, qs, qs, color, 0, 0, 12);

  // painel de status
  app.box(16, 300, 468, 60, 0x1A2230FF, 1, 0x33445566 & 0xFFFFFFFF, 8);
  app.text(30, 312, "dragging=" + input.dragging(app._win) + "  double-clicks=" + doubles + "  releases=" + releases, 0x99CCFFFF, 14);
  app.text(30, 334, "mouse: " + mx + "," + my + "  delta: " + input.mouseDeltaX(app._win) + "," + input.mouseDeltaY(app._win), 0x808890FF, 13);

  app.endFrame();
}

app.close();
