import egui from "rts:egui";
import dom from "rts:dom";

// PAINEL DE PERFIL DE USUÁRIO — um "sistema" real renderizado pelo motor HTML/egui
// no pipeline unificado (DOM no rts-dom; egui só LÊ e pinta). Tem: header com nome
// + cargo + status, cards de estatísticas (lado a lado), bloco de infos, lista de
// atividade recente, e uma "badge" estilizada inline. Layout via box model +
// unidades (%/px), estilo por tag + style="" inline.
//   target/release/rts.exe run examples/claude-egui-perfil-usuario.ts

const COLOR = 0, BG = 1, FONT = 2, PAD = 3, MARGIN = 4, BW = 5, BC = 6, RADIUS = 7, WIDTH = 8;
const DR = 1000000000;
const pct = (p: number): number => 1 * DR + p * 1000;

// ── Layout (0=vertical 1=wrap 2=horizontal 3=grid) ────────────────────────────
dom.defineBlock("app", 0, 0, 0, 0);
dom.defineBlock("header", 2, 0, 0, 0);   // horizontal: avatar | nome
dom.defineBlock("avatar", 0, 0, 0, 0);
dom.defineBlock("ident", 0, 0, 0, 0);
dom.defineBlock("name", 0, 26, 0, 2);
dom.defineBlock("role", 1, 0, 0, 0);
dom.defineBlock("stats", 2, 0, 0, 0);    // horizontal: 4 cards
dom.defineBlock("stat", 0, 0, 0, 4);
dom.defineBlock("statnum", 0, 24, 0, 0);
dom.defineBlock("statlbl", 1, 0, 0, 0);
dom.defineBlock("panel", 0, 0, 0, 0);
dom.defineBlock("h2", 0, 18, 0, 4);
dom.defineBlock("row", 2, 0, 0, 0);      // horizontal: label | valor
dom.defineBlock("k", 1, 0, 0, 0);
dom.defineBlock("v", 1, 0, 0, 0);
dom.defineBlock("ul", 0, 0, 0, 0);
dom.defineBlock("li", 1, 0, 1, 0);
dom.defineBlock("p", 1, 0, 0, 0);
dom.defineInline("b", 8);
dom.defineInline("i", 16);

// ── Estilo (tema escuro de dashboard) ─────────────────────────────────────────
dom.defineStyle("header", BG, 0x14233CFF);
dom.defineStyle("header", PAD, 16);
dom.defineStyle("header", RADIUS, 12);
dom.defineStyle("header", MARGIN, 8);
dom.defineStyle("header", BW, 1);
dom.defineStyle("header", BC, 0x2E4A70FF);
dom.defineStyle("header", WIDTH, pct(96));

// avatar = quadrado colorido (placeholder de imagem)
dom.defineStyle("avatar", BG, 0x3A6EA5FF);
dom.defineStyle("avatar", WIDTH, pct(18));
dom.defineStyle("avatar", PAD, 26);
dom.defineStyle("avatar", RADIUS, 40);
dom.defineStyle("avatar", MARGIN, 6);

dom.defineStyle("name", COLOR, 0xEAF2FBFF);
dom.defineStyle("name", FONT, 26);
dom.defineStyle("role", COLOR, 0x88AACCFF);
dom.defineStyle("role", FONT, 14);

// cards de estatística
dom.defineStyle("stat", BG, 0x101D30FF);
dom.defineStyle("stat", PAD, 12);
dom.defineStyle("stat", RADIUS, 10);
dom.defineStyle("stat", MARGIN, 5);
dom.defineStyle("stat", BW, 1);
dom.defineStyle("stat", BC, 0x294166FF);
dom.defineStyle("stat", WIDTH, pct(22));
dom.defineStyle("statnum", FONT, 24);
dom.defineStyle("statlbl", COLOR, 0x8092A8FF);
dom.defineStyle("statlbl", FONT, 12);

// painéis (infos / atividade)
dom.defineStyle("panel", BG, 0x101D30FF);
dom.defineStyle("panel", PAD, 16);
dom.defineStyle("panel", RADIUS, 12);
dom.defineStyle("panel", MARGIN, 8);
dom.defineStyle("panel", WIDTH, pct(96));
dom.defineStyle("h2", COLOR, 0x9FC8F0FF);
dom.defineStyle("h2", FONT, 18);
dom.defineStyle("k", COLOR, 0x8092A8FF);
dom.defineStyle("k", FONT, 14);
dom.defineStyle("k", WIDTH, pct(30));
dom.defineStyle("v", COLOR, 0xD8E2F0FF);
dom.defineStyle("v", FONT, 14);
dom.defineStyle("li", COLOR, 0xC0CCDAFF);
dom.defineStyle("li", FONT, 14);
dom.defineStyle("p", COLOR, 0xC8D2E0FF);
dom.defineStyle("p", FONT, 14);

const HTML =
  "<app>" +
    "<header>" +
      "<avatar><b>MA</b></avatar>" +
      "<ident>" +
        "<name>Marcos Andrade</name>" +
        "<role>Engenheiro de Software · " +
        "<i style=\"color:#66DD99\">online</i> · membro desde 2024</role>" +
      "</ident>" +
    "</header>" +

    "<stats>" +
      "<stat><statnum style=\"color:#66CCFF\">128</statnum><statlbl>projetos</statlbl></stat>" +
      "<stat><statnum style=\"color:#66DD99\">1.7k</statnum><statlbl>commits</statlbl></stat>" +
      "<stat><statnum style=\"color:#FFCC66\">342</statnum><statlbl>PRs</statlbl></stat>" +
      "<stat><statnum style=\"color:#FF99CC\">89%%</statnum><statlbl>cobertura</statlbl></stat>" +
    "</stats>" +

    "<panel>" +
      "<h2>Informacoes</h2>" +
      "<row><k>Email</k><v>marcos@rts.dev</v></row>" +
      "<row><k>Funcao</k><v>Admin · acesso total</v></row>" +
      "<row><k>Equipe</k><v>Core / Compilador</v></row>" +
      "<row><k>Localizacao</k><v>Brasil (UTC-3)</v></row>" +
      "<row><k>Plano</k><v><b style=\"color:#FFD080\">Pro</b> — renova em 12 dias</v></row>" +
    "</panel>" +

    "<panel>" +
      "<h2>Atividade recente</h2>" +
      "<ul>" +
        "<li><b>fez merge</b> de feat/dom-f2-box-model — <i>ha 2 min</i></li>" +
        "<li><b>criou</b> a branch rts-input dedicada — <i>ha 1 h</i></li>" +
        "<li><b>comentou</b> em #1742 (DOM headless) — <i>ha 3 h</i></li>" +
        "<li><b>compilou</b> pong.exe via AOT (8.9 MB) — <i>ontem</i></li>" +
      "</ul>" +
      "<p style=\"color:#7F90A8; font-size:12\">Renderizado 100%% pelo motor HTML do RTS " +
      "(DOM nativo em Rust, headless) — o egui so le o DOM e pinta.</p>" +
    "</panel>" +
  "</app>";

const win = egui.openWindow("Perfil do Usuario — RTS Dashboard", 880, 720, 0);
const d = dom.parseHtml(HTML); // DOM no rts-dom (headless); egui so LE e pinta.

while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  egui.beginFrame(win);
  egui.render(win, d);
  egui.endFrame(win);
}

egui.close(win);
dom.free(d);
