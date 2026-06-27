import egui from "rts:egui";
import dom from "rts:dom";

// F2 — box model (margin/padding/border/bg/raio) + WIDTH com TODAS as unidades
// (px/%/em/rem/vw/vh/auto) via SLOT OPACO ou style="" inline. O Rust nunca casa
// string CSS (invariante 4): o TS mapeia nome-CSS → slot/faixa. As unidades
// relativas resolvem TARDE no render (north-star risco 5): % contra o content-box
// do PAI, vw/vh contra a viewport, em/rem contra o font-size.
//   target/release/rts.exe run examples/claude-egui-box-model.ts

const SLOT_COLOR = 0;
const SLOT_BG = 1;
const SLOT_FONT_SIZE = 2;
const SLOT_PADDING = 3;
const SLOT_BORDER_WIDTH = 5;
const SLOT_BORDER_COLOR = 6;
const SLOT_CORNER_RADIUS = 7;
const SLOT_WIDTH = 8;

// Codificação ABI da Dimension (style.rs): faixa por unidade, valor × 1000.
const DIM_RANGE = 1000000000;
const widthPercent = (p: number): number => 1 * DIM_RANGE + p * 1000; // base %
const widthVw = (v: number): number => 4 * DIM_RANGE + v * 1000; // base vw

dom.defineBlock("h1", 0, 24, 0, 6);
dom.defineBlock("div", 1, 0, 0, 4);

dom.defineStyle("h1", SLOT_COLOR, 0xE8EEF5FF);
dom.defineStyle("h1", SLOT_FONT_SIZE, 26);

// .card via TAG div: caixa azul com padding/borda/raio. width 70% (do pai).
dom.defineStyle("div", SLOT_BG, 0x1A2A44FF);
dom.defineStyle("div", SLOT_PADDING, 14);
dom.defineStyle("div", SLOT_BORDER_WIDTH, 2);
dom.defineStyle("div", SLOT_BORDER_COLOR, 0x4A7AC0FF);
dom.defineStyle("div", SLOT_CORNER_RADIUS, 10);
dom.defineStyle("div", SLOT_WIDTH, widthPercent(70)); // width: 70% (do content-box do pai)

const win = egui.openWindow("F2 — box model + unidades", 560, 420, 0);

// 4 caixas demonstrando unidades: 70% (tag), 280px (inline), 50vw (inline),
// auto (inline). Cada style="" inline SOBREPÕE o 70% da tag.
const HTML =
  "<h1>Unidades de largura</h1>" +
  "<div>70% da largura do pai (vem da tag div).</div>" +
  "<div style=\"width: 280px; background-color: #2A1A30; border-color: #C06AA0\">" +
  "280px fixos (inline).</div>" +
  "<div style=\"width: 50vw; background-color: #1A3020; border-color: #6AC080\">" +
  "50vw = metade da largura da JANELA (viewport).</div>" +
  "<div style=\"width: auto; background-color: #30281A; border-color: #C0A06A\">" +
  "auto = ocupa a largura disponivel.</div>";

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
