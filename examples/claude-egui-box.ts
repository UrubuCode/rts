import egui from "rts:egui";
import dom from "rts:dom";

// F2 — box model de bloco via egui::Frame: bg + padding + margin + border + raio.
// Slots opacos (invariante 4): 0=color 1=bg 2=font_size 3=padding 4=margin
// 5=border_width 6=border_color 7=corner_radius. Cores 0xRRGGBBAA em i64.
//
// Critério: um "card" com fundo escuro, padding interno, borda e cantos
// arredondados, 100% via egui::Frame (zero painter absoluto). Rodar:
//   target/release/rts.exe run examples/claude-egui-box.ts

const COLOR = 0, BG = 1, FONT = 2, PAD = 3, MARGIN = 4, BW = 5, BC = 6, RADIUS = 7;

dom.defineBlock("h1", 0, 22, 0, 4);
dom.defineBlock("p", 1, 0, 0, 0);
dom.defineBlock("div", 0, 0, 0, 0); // card = bloco vertical

// O card: fundo, padding, margem externa, borda azul, cantos arredondados.
dom.defineStyle("div", BG, 0x1E2530FF);
dom.defineStyle("div", PAD, 16);
dom.defineStyle("div", MARGIN, 10);
dom.defineStyle("div", BW, 2);
dom.defineStyle("div", BC, 0x0088FFFF);
dom.defineStyle("div", RADIUS, 10);

// Tipografia dentro do card.
dom.defineStyle("h1", COLOR, 0xFFFFFFFF);
dom.defineStyle("p", COLOR, 0xB0B8C0FF);

const win = egui.openWindow("F2 — box model (card)", 460, 280, 0);

const HTML =
  "<div>" +
  "<h1>Card com caixa</h1>" +
  "<p>Fundo, padding, borda azul e cantos arredondados — tudo via egui::Frame, " +
  "controlado por slots opacos do defineStyle.</p>" +
  "</div>";

// DOM no rts-dom (headless); o egui só LÊ e pinta via egui.render.
const d = dom.parseHtml(HTML);

while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  egui.beginFrame(win);
  egui.render(win, d);
  egui.endFrame(win);
}

egui.close(win);
dom.free(d);
