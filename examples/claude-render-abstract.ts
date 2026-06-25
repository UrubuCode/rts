import egui from "rts:egui";
import render from "rts:render";

// PoC do RENDER ABSTRATO: a PINTURA usa o namespace `render` genérico (não
// egui.draw*). O egui é apenas o BACKEND ATIVO que implementa render.* — trocar
// de backend não muda este código. A janela/loop ainda vêm do egui (gerência de
// janela); a PINTURA é abstrata. Ver docs/specs/dom-render-input-interfaces.md.
//   target/release/rts.exe run examples/claude-render-abstract.ts

const win = egui.openWindow("Render abstrato (backend plugavel)", 480, 300, 0);
egui.moveWindow(win, 2200, 400); // UI na tela 2

const BG = 0x12161CFF;
const CARD = 0x1E2A3AFF;
const BORDER = 0x33CC88FF;
const WHITE = 0xFFFFFFFF;
const GRAY = 0xB0B8C0FF;

while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;

  // ciclo de frame VIA RENDER (não egui)
  render.beginFrame(win);

  render.rect(win, 0, 0, 480, 300, BG, 0, 0, 0);
  render.rect(win, 24, 24, 432, 120, CARD, 2, BORDER, 12);
  render.text(win, 40, 40, "Pintado via render.* (nao egui.draw)", WHITE, 20, 0);

  // measureText abstrato: o layout mede pelo namespace render
  const label = "medido por render:";
  const w = render.measureText(win, label, 16, 0);
  render.text(win, 40, 78, label, GRAY, 16, 0);
  render.text(win, 40 + w + 8, 78, "" + w, BORDER, 16, 0);

  render.text(win, 40, 104, "egui e so o backend ativo desta interface.", GRAY, 16, 0);
  render.line(win, 24, 170, 456, 170, 2, BORDER);

  render.endFrame(win);
}

egui.close(win);
