import egui from "rts:egui";
import dom from "rts:dom";

// PIPELINE UNIFICADO (headless-puro): o DOM vive 100% no rts-dom; o egui é um
// RENDER GENÉRICO que só LÊ esse DOM e pinta. Não há `egui.html` (que detinha um
// DOM próprio) — aqui o DOM é do rts-dom (dom.parseHtml → handle), estilizado via
// dom.defineStyle, e o egui.render(win, d) pinta o MESMO DOM. Trocar o backend de
// render (futuro: web/png) é trocar só quem consome `d`.
//   target/release/rts.exe run examples/claude-dom-render-unificado.ts

const COLOR = 0, BG = 1, FONT = 2, PAD = 3, BW = 5, BC = 6, RADIUS = 7;

// estilo/layout DEFINIDOS NO DOM (rts-dom), não no egui.
dom.defineBlock("h1", 0, 24, 0, 6);
dom.defineBlock("div", 0, 0, 0, 4);
dom.defineBlock("p", 1, 0, 0, 0);
dom.defineStyle("h1", COLOR, 0x66CCFFFF);
dom.defineStyle("h1", FONT, 26);
dom.defineStyle("div", BG, 0x1A2A44FF);
dom.defineStyle("div", PAD, 14);
dom.defineStyle("div", BW, 2);
dom.defineStyle("div", BC, 0x4A7AC0FF);
dom.defineStyle("div", RADIUS, 10);
dom.defineStyle("p", COLOR, 0xC8D2E0FF);
dom.defineStyle("p", FONT, 15);

// o DOM é do rts-dom (headless — manipulável sem janela).
const d = dom.parseHtml(
  "<h1>DOM no rts-dom</h1>" +
  "<div><p>Esta arvore vive no rts-dom (headless). O egui apenas a LE e pinta " +
  "via egui.render(win, d) — render generico, DOM desacoplado.</p></div>"
);

const win = egui.openWindow("Pipeline unificado — DOM no rts-dom, egui so renderiza", 600, 320, 0);

while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  egui.beginFrame(win);
  egui.render(win, d); // ← o egui LÊ o DOM `d` do rts-dom e pinta
  egui.endFrame(win);
}

egui.close(win);
dom.free(d);
