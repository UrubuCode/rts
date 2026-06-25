import egui from "rts:egui";

// PoC do CANVAS BURRO: o TS calcula posições e cores e emite primitivos de
// pintura (drawRect/drawText/drawLine); o egui SÓ pinta. measureText devolve a
// largura pra o TS decidir layout. É a base da arquitetura "DOM/layout em TS,
// egui só pinta". Ver docs/specs/dom-in-ts-architecture.md.
//   target/release/rts.exe run examples/claude-canvas-poc.ts

const win = egui.openWindow("Canvas burro — TS dirige, egui pinta", 520, 360, 0);

// cores 0xRRGGBBAA
const BG = 0x12161CFF;
const CARD = 0x1E2A3AFF;
const BORDER = 0x3399FFFF;
const WHITE = 0xFFFFFFFF;
const GRAY = 0xB0B8C0FF;
const ACCENT = 0xFF8800FF;

while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  egui.beginFrame(win);

  // fundo
  egui.drawRect(win, 0, 0, 520, 360, BG, 0, 0, 0);

  // um "card" calculado pelo TS: posição/tamanho são do TS, não do egui
  const cardX = 24;
  const cardY = 24;
  const cardW = 472;
  egui.drawRect(win, cardX, cardY, cardW, 120, CARD, 2, BORDER, 12);

  // título dentro do card (posição relativa calculada em TS)
  const title = "Canvas burro funciona";
  egui.drawText(win, cardX + 16, cardY + 16, title, WHITE, 24, 0);

  // TS MEDE o texto pra posicionar algo logo depois dele
  const titleW = egui.measureText(win, title, 24, 0);
  egui.drawText(win, cardX + 16 + titleW + 12, cardY + 22, "(medido!)", ACCENT, 14, 0);

  egui.drawText(win, cardX + 16, cardY + 56, "O TS calculou todas as posicoes.", GRAY, 16, 0);
  egui.drawText(win, cardX + 16, cardY + 80, "O egui so pintou retangulos e texto.", GRAY, 16, 0);

  // uma régua de barras cuja largura o TS calcula (layout em TS)
  let i = 0;
  while (i < 5) {
    const bx = cardX + i * 92;
    const bw = 80;
    const bh = 18 + i * 12;
    egui.drawRect(win, bx, 180, bw, bh, BORDER, 0, 0, 4);
    i = i + 1;
  }

  // uma linha separadora
  egui.drawLine(win, 24, 300, 496, 300, 2, BORDER);
  egui.drawText(win, 24, 312, "drawRect + drawText + drawLine + measureText", GRAY, 14, 0);

  egui.endFrame(win);
}

egui.close(win);
