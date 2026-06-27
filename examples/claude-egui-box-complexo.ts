import egui from "rts:egui";
import dom from "rts:dom";

// F2 — teste COMPLEXO do box model: caixas aninhadas, padding/margin/borda/raio
// variados, cores distintas, headings e parágrafos dentro. Estressa o egui::Frame.
//
// Slots opacos: 0=color 1=bg 2=font_size 3=padding 4=margin 5=border_width
// 6=border_color 7=corner_radius. Cores 0xRRGGBBAA em i64.
//   target/release/rts.exe run examples/claude-egui-box-complexo.ts

const COLOR = 0, BG = 1, FONT = 2, PAD = 3, MARGIN = 4, BW = 5, BC = 6, RADIUS = 7;

// ── Layout das tags (todas blocos verticais; tags próprias são data-driven) ─────
dom.defineBlock("page", 0, 0, 0, 0);
dom.defineBlock("card", 0, 0, 0, 0);
dom.defineBlock("inner", 0, 0, 0, 0);
dom.defineBlock("danger", 0, 0, 0, 0);
dom.defineBlock("ok", 0, 0, 0, 0);
dom.defineBlock("h1", 0, 24, 0, 4);
dom.defineBlock("h2", 0, 18, 0, 4);
dom.defineBlock("p", 1, 0, 0, 0);
dom.defineInline("b", 8);
// row = container HORIZONTAL (display 2): os filhos ficam lado a lado, na MESMA
// linha. tile = mini-card dentro da row.
dom.defineBlock("row", 2, 0, 0, 0);
dom.defineBlock("tile", 0, 0, 0, 0);

// ── page: fundo geral, padding largo ───────────────────────────────────────────
dom.defineStyle("page", BG, 0x12161CFF);
dom.defineStyle("page", PAD, 18);

// ── card: caixa azul com borda e cantos médios, margem entre cards ──────────────
dom.defineStyle("card", BG, 0x1E2A3AFF);
dom.defineStyle("card", PAD, 14);
dom.defineStyle("card", MARGIN, 8);
dom.defineStyle("card", BW, 2);
dom.defineStyle("card", BC, 0x3399FFFF);
dom.defineStyle("card", RADIUS, 12);

// ── inner: caixa aninhada DENTRO do card, padding menor, cantos pequenos ────────
dom.defineStyle("inner", BG, 0x0E1620FF);
dom.defineStyle("inner", PAD, 10);
dom.defineStyle("inner", MARGIN, 6);
dom.defineStyle("inner", RADIUS, 6);

// ── danger: card vermelho, borda grossa, cantos retos (raio 0) ──────────────────
dom.defineStyle("danger", BG, 0x3A1E1EFF);
dom.defineStyle("danger", PAD, 12);
dom.defineStyle("danger", MARGIN, 8);
dom.defineStyle("danger", BW, 3);
dom.defineStyle("danger", BC, 0xFF4444FF);
dom.defineStyle("danger", RADIUS, 0);

// ── ok: card verde, borda fina, cantos bem arredondados ─────────────────────────
dom.defineStyle("ok", BG, 0x14301EFF);
dom.defineStyle("ok", PAD, 12);
dom.defineStyle("ok", MARGIN, 8);
dom.defineStyle("ok", BW, 1);
dom.defineStyle("ok", BC, 0x44DD66FF);
dom.defineStyle("ok", RADIUS, 18);

// ── tipografia ──────────────────────────────────────────────────────────────────
dom.defineStyle("h1", COLOR, 0xFFFFFFFF);
dom.defineStyle("h2", COLOR, 0x99CCFFFF);
dom.defineStyle("p", COLOR, 0xC0C8D0FF);

// ── tile: 3 mini-caixas LADO A LADO (dentro da row horizontal) ──────────────────
dom.defineStyle("tile", BG, 0x223044FF);
dom.defineStyle("tile", PAD, 10);
dom.defineStyle("tile", MARGIN, 6);
dom.defineStyle("tile", BW, 1);
dom.defineStyle("tile", BC, 0x66AAFFFF);
dom.defineStyle("tile", RADIUS, 8);

const win = egui.openWindow("F2 — caixas aninhadas (box model)", 560, 640, 0);

const HTML =
  "<page>" +
    "<h1>Dashboard de caixas</h1>" +
    "<card>" +
      "<h2>Card externo (azul, raio 12)</h2>" +
      "<p>Este card tem fundo, padding 14, borda azul e cantos arredondados.</p>" +
      "<inner>" +
        "<p>Caixa <b>aninhada</b> aqui dentro — fundo mais escuro, padding 10, " +
        "cantos 6. Prova que Frame dentro de Frame compoe.</p>" +
      "</inner>" +
      "<inner>" +
        "<p>Segunda caixa aninhada, com margem entre elas.</p>" +
      "</inner>" +
    "</card>" +
    "<danger>" +
      "<h2>Alerta (vermelho, borda 3, cantos retos)</h2>" +
      "<p>Borda grossa e raio 0 — uma caixa de aviso bem marcada.</p>" +
    "</danger>" +
    "<ok>" +
      "<h2>Sucesso (verde, borda fina, raio 18)</h2>" +
      "<p>Cantos bem arredondados e borda fina — estilo pill.</p>" +
      "<inner>" +
        "<p>Nota aninhada dentro do card verde.</p>" +
      "</inner>" +
    "</ok>" +
    "<h2>Tres caixas na MESMA linha (row horizontal)</h2>" +
    "<row>" +
      "<tile><p>Caixa A</p></tile>" +
      "<tile><p>Caixa B</p></tile>" +
      "<tile><p>Caixa C</p></tile>" +
    "</row>" +
    "<p style=\"color:#FFD700\">Rodape com cor inline (style=) sobrepondo o default.</p>" +
  "</page>";

// Parseia uma vez e DUMPA a árvore (devtools-style) — prova que o DOM aninhado
// (page > card > inner > p > b ...) ficou correto, sem depender de ver os pixels.
egui.beginFrame(win);
egui.render(win, d);
dom.dump(win);
egui.endFrame(win);

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
