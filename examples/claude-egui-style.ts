import egui from "rts:egui";
import dom from "rts:dom";

// F1 — estilo de texto por SLOT OPACO (defineStyle). O TS mapeia nome-CSS → índice;
// o Rust nunca casa string CSS (invariante 4). Cores como 0xRRGGBBAA em i64.
//
// Slots (contrato com o Rust): 0=color, 1=bg, 2=font_size.
// Critério de sucesso: h1 AZUL tamanho 28 + p CINZA, 100% via egui RichText
// (zero painter absoluto). Rodar:
//   target/release/rts.exe run examples/claude-egui-style.ts

const SLOT_COLOR = 0;
const SLOT_BG = 1;
const SLOT_FONT_SIZE = 2;

// Layout/estilo DEFINIDOS NO DOM (rts-dom), não no egui.
dom.defineBlock("h1", 0, 26, 0, 4); // heading
dom.defineBlock("p", 1, 0, 0, 0); // parágrafo (wrap)
// Estilo por TAG — acumula slots (cor + tamanho na mesma tag).
dom.defineStyle("h1", SLOT_COLOR, 0x0088FFFF); // h1 azul
dom.defineStyle("h1", SLOT_FONT_SIZE, 28); // tamanho 28
dom.defineStyle("p", SLOT_COLOR, 0xCCCCCCFF); // p cinza claro

// O DOM vive no rts-dom (headless); o egui só LÊ e pinta via egui.render(win, d).
const d = dom.parseHtml(
  "<h1>Titulo azul tamanho 28</h1>" +
  "<p>Paragrafo cinza claro, estilizado via defineStyle (slot opaco). " +
  "O <b>negrito</b> herda a cor do paragrafo.</p>"
);

const win = egui.openWindow("F1 — estilo por slot opaco", 460, 220, 0);
while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  egui.beginFrame(win);
  egui.render(win, d);
  egui.endFrame(win);
}

egui.close(win);
dom.free(d);
