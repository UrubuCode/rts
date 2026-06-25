import egui from "rts:egui";

// F2 — teste COMPLEXO do box model: caixas aninhadas, padding/margin/borda/raio
// variados, cores distintas, headings e parágrafos dentro. Estressa o egui::Frame.
//
// Slots opacos: 0=color 1=bg 2=font_size 3=padding 4=margin 5=border_width
// 6=border_color 7=corner_radius. Cores 0xRRGGBBAA em i64.
//   target/release/rts.exe run examples/claude-egui-box-complexo.ts

const COLOR = 0, BG = 1, FONT = 2, PAD = 3, MARGIN = 4, BW = 5, BC = 6, RADIUS = 7;

// ── Layout das tags (todas blocos verticais; tags próprias são data-driven) ─────
egui.defineBlock("page", 0, 0, 0, 0);
egui.defineBlock("card", 0, 0, 0, 0);
egui.defineBlock("inner", 0, 0, 0, 0);
egui.defineBlock("danger", 0, 0, 0, 0);
egui.defineBlock("ok", 0, 0, 0, 0);
egui.defineBlock("h1", 0, 24, 0, 4);
egui.defineBlock("h2", 0, 18, 0, 4);
egui.defineBlock("p", 1, 0, 0, 0);
egui.defineInline("b", 8);
// row = container HORIZONTAL (display 2): os filhos ficam lado a lado, na MESMA
// linha. tile = mini-card dentro da row.
egui.defineBlock("row", 2, 0, 0, 0);
egui.defineBlock("tile", 0, 0, 0, 0);

// ── page: fundo geral, padding largo ───────────────────────────────────────────
egui.defineStyle("page", BG, 0x12161CFF);
egui.defineStyle("page", PAD, 18);

// ── card: caixa azul com borda e cantos médios, margem entre cards ──────────────
egui.defineStyle("card", BG, 0x1E2A3AFF);
egui.defineStyle("card", PAD, 14);
egui.defineStyle("card", MARGIN, 8);
egui.defineStyle("card", BW, 2);
egui.defineStyle("card", BC, 0x3399FFFF);
egui.defineStyle("card", RADIUS, 12);

// ── inner: caixa aninhada DENTRO do card, padding menor, cantos pequenos ────────
egui.defineStyle("inner", BG, 0x0E1620FF);
egui.defineStyle("inner", PAD, 10);
egui.defineStyle("inner", MARGIN, 6);
egui.defineStyle("inner", RADIUS, 6);

// ── danger: card vermelho, borda grossa, cantos retos (raio 0) ──────────────────
egui.defineStyle("danger", BG, 0x3A1E1EFF);
egui.defineStyle("danger", PAD, 12);
egui.defineStyle("danger", MARGIN, 8);
egui.defineStyle("danger", BW, 3);
egui.defineStyle("danger", BC, 0xFF4444FF);
egui.defineStyle("danger", RADIUS, 0);

// ── ok: card verde, borda fina, cantos bem arredondados ─────────────────────────
egui.defineStyle("ok", BG, 0x14301EFF);
egui.defineStyle("ok", PAD, 12);
egui.defineStyle("ok", MARGIN, 8);
egui.defineStyle("ok", BW, 1);
egui.defineStyle("ok", BC, 0x44DD66FF);
egui.defineStyle("ok", RADIUS, 18);

// ── tipografia ──────────────────────────────────────────────────────────────────
egui.defineStyle("h1", COLOR, 0xFFFFFFFF);
egui.defineStyle("h2", COLOR, 0x99CCFFFF);
egui.defineStyle("p", COLOR, 0xC0C8D0FF);

// ── tile: 3 mini-caixas LADO A LADO (dentro da row horizontal) ──────────────────
egui.defineStyle("tile", BG, 0x223044FF);
egui.defineStyle("tile", PAD, 10);
egui.defineStyle("tile", MARGIN, 6);
egui.defineStyle("tile", BW, 1);
egui.defineStyle("tile", BC, 0x66AAFFFF);
egui.defineStyle("tile", RADIUS, 8);

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
egui.html(win, HTML);
egui.domDump(win);
egui.endFrame(win);

while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  egui.beginFrame(win);
  egui.html(win, HTML);
  egui.endFrame(win);
}

egui.close(win);
