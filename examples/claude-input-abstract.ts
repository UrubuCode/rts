import egui from "rts:egui";
import render from "rts:render";
import input from "rts:input";

// PoC do INPUT ABSTRATO: o programa lê mouse/clique via `input.*` (não egui) e
// reage. O egui é o backend ativo que CAPTA o input cru; o programa (papel do
// DOM/layout) INTERPRETA — hit-test (o quadrado contem o mouse?) e dispara a ação
// (conta clique). Pinta via `render.*`. Ver docs/specs/dom-render-input-interfaces.md.
//   target/release/rts.exe run examples/claude-input-abstract.ts

const win = egui.openWindow("Input abstrato (mouse/clique via input.*)", 480, 320, 0);

const BG = 0x12161CFF;
const IDLE = 0x33446688 & 0xFFFFFFFF;
const HOVER = 0x33CC88FF;
const PRESSED = 0xFF8800FF;
const WHITE = 0xFFFFFFFF;
const GRAY = 0xB0B8C0FF;

// um "botao" cujo retangulo o programa conhece (hit-test em TS)
const bx = 160;
const by = 120;
const bw = 160;
const bh = 70;

let clicks = 0;

while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;

  render.beginFrame(win);

  // LÊ o input cru (polling) — DENTRO do frame (o beginFrame alimenta o input do
  // backend). Papel do backend reportar; do programa interpretar.
  const mx = input.mouseX(win);
  const my = input.mouseY(win);
  const clicked = input.mouseClicked(win, 0);
  const down = input.mouseDown(win, 0);

  // HIT-TEST em TS: o mouse esta sobre o botao?
  const over = mx >= bx && mx <= bx + bw && my >= by && my <= by + bh;
  if (over && clicked !== 0) {
    clicks = clicks + 1;
  }

  // escolhe a cor pelo estado (hover/pressed)
  let fill = IDLE;
  if (over) {
    fill = HOVER;
    if (down !== 0) fill = PRESSED;
  }

  render.rect(win, 0, 0, 480, 320, BG, 0, 0, 0);
  render.rect(win, bx, by, bw, bh, fill, 2, WHITE, 10);
  render.text(win, bx + 24, by + 26, "Clique aqui", WHITE, 18, 0);
  render.text(win, 24, 24, "Mouse: " + mx + ", " + my, GRAY, 14, 0);
  render.text(win, 24, 44, "Cliques no botao: " + clicks, WHITE, 16, 0);
  render.text(win, 24, 280, "input.* le o mouse; o hit-test e a contagem sao do programa.", GRAY, 13, 0);
  render.endFrame(win);
}

egui.close(win);
