import egui from "rts:egui";
import dom from "rts:dom";

// HTML RICO — teste visual do pipeline unificado (DOM no rts-dom + egui.render).
// Landing page com hero, cards aninhados, citação, lista, várias unidades (px/%/
// vw/em), aninhamento profundo, override inline. O DOM vive 100% no rts-dom
// (headless); o egui só LÊ e pinta. Trocar o backend = trocar quem consome `d`.
//   target/release/rts.exe run examples/claude-egui-html-rico.ts

const COLOR = 0, BG = 1, FONT = 2, PAD = 3, MARGIN = 4, BW = 5, BC = 6, RADIUS = 7, WIDTH = 8;
const DR = 1000000000;
const pct = (p: number): number => 1 * DR + p * 1000;
const vw = (v: number): number => 4 * DR + v * 1000;
const em = (e: number): number => 2 * DR + e * 1000;

// ── Layout (display: 0=vertical 1=wrap 2=horizontal 3=grid) ───────────────────
dom.defineBlock("body", 0, 0, 0, 0);
dom.defineBlock("hero", 0, 0, 0, 0);
dom.defineBlock("h1", 0, 34, 0, 4);
dom.defineBlock("h2", 0, 22, 0, 4);
dom.defineBlock("h3", 0, 17, 0, 2);
dom.defineBlock("tagline", 1, 0, 0, 0);
dom.defineBlock("grid", 2, 0, 0, 0);   // horizontal (cards lado a lado)
dom.defineBlock("card", 0, 0, 0, 6);
dom.defineBlock("section", 0, 0, 0, 0);
dom.defineBlock("quote", 0, 12, 0, 0); // recuo (blockquote-like)
dom.defineBlock("ul", 0, 0, 0, 0);
dom.defineBlock("li", 1, 0, 1, 0);
dom.defineBlock("p", 1, 0, 0, 0);
dom.defineInline("b", 8);
dom.defineInline("i", 16);
dom.defineInline("code", 1);

// ── Estilo por TAG ────────────────────────────────────────────────────────────
dom.defineStyle("hero", BG, 0x0B1A2EFF);
dom.defineStyle("hero", PAD, 22);
dom.defineStyle("hero", RADIUS, 14);
dom.defineStyle("hero", MARGIN, 8);
dom.defineStyle("hero", WIDTH, vw(96));
dom.defineStyle("h1", COLOR, 0x7FD0FFFF);
dom.defineStyle("h1", FONT, 34);
dom.defineStyle("h2", COLOR, 0xAAD4FFFF);
dom.defineStyle("h2", FONT, 22);
dom.defineStyle("h3", COLOR, 0x9FC0E0FF);
dom.defineStyle("tagline", COLOR, 0x88AACCFF);
dom.defineStyle("tagline", FONT, 14);

dom.defineStyle("card", BG, 0x14233CFF);
dom.defineStyle("card", PAD, 14);
dom.defineStyle("card", BW, 1);
dom.defineStyle("card", BC, 0x2E4A70FF);
dom.defineStyle("card", RADIUS, 10);
dom.defineStyle("card", MARGIN, 6);
dom.defineStyle("card", WIDTH, pct(32)); // 3 cards ~ lado a lado

dom.defineStyle("section", BG, 0x101D30FF);
dom.defineStyle("section", PAD, 16);
dom.defineStyle("section", MARGIN, 8);
dom.defineStyle("section", RADIUS, 12);
dom.defineStyle("section", WIDTH, pct(96));

dom.defineStyle("quote", BG, 0x1A2A1EFF);
dom.defineStyle("quote", PAD, 12);
dom.defineStyle("quote", BW, 1);
dom.defineStyle("quote", BC, 0x3A6A4AFF);
dom.defineStyle("quote", RADIUS, 8);
dom.defineStyle("quote", COLOR, 0xA8E0B8FF);
dom.defineStyle("quote", WIDTH, pct(80));

dom.defineStyle("p", COLOR, 0xC8D2E0FF);
dom.defineStyle("p", FONT, 15);
dom.defineStyle("li", COLOR, 0xB0E0C0FF);
dom.defineStyle("li", FONT, 15);

const HTML =
  "<body>" +
    "<hero>" +
      "<h1>RTS — DOM nativo headless</h1>" +
      "<tagline>Manipule HTML/CSS sem browser, sem jsdom. Node e Bun nao tem isto " +
      "nativo. O DOM vive no <b>rts-dom</b> (Rust); o egui so LE e pinta.</tagline>" +
    "</hero>" +

    "<section>" +
      "<h2>Por que importa</h2>" +
      "<grid>" +
        "<card><h3>Headless</h3><p>Parse, query e mutacao <b>sem janela</b>. SSR, scraping, templating direto no runtime.</p></card>" +
        "<card><h3>Nativo</h3><p>Arvore em arena Rust. Rapido, sem libs JS (<i>jsdom</i>/<i>linkedom</i>).</p></card>" +
        "<card><h3>Desacoplado</h3><p>Render plugavel: <code>egui.render(win, d)</code> hoje; web/png amanha. Mesmo <b>d</b>.</p></card>" +
      "</grid>" +
    "</section>" +

    "<section>" +
      "<h2>Box model + unidades</h2>" +
      "<ul>" +
        "<li>Caixas: <b>bg</b>, padding, borda, raio, margem (egui::Frame)</li>" +
        "<li>Largura: <b>px</b> 280, <b>%</b> do pai, <b>vw</b> da viewport, <b>em</b>/rem da fonte</li>" +
        "<li>Estilo em 3 camadas: tag &lt; <i>style=\"\"</i> inline &lt; setStyleBatch por-no</li>" +
        "<li>DOM core: parentNode, firstChild, nextSibling, classList, createTextNode, comentarios</li>" +
      "</ul>" +
      "<div style=\"background-color:#2A1E14; padding:10; border-radius:8; border-color:#C0905A; width:50%\">" +
        "<p style=\"color:#FFCC88; font-size:13\">Esta caixa usa <b>width:50%</b> + estilo 100%% inline " +
        "(o style=\"\" sobrepoe a tag). O % resolve tarde contra o content-box do pai.</p>" +
      "</div>" +
    "</section>" +

    "<section>" +
      "<h2>Nota</h2>" +
      "<quote><p>“O DOM e o conteudo; o render e detalhe. Isolar o DOM no rts-dom " +
      "deixa a pagina <b>renderizavel por qualquer backend</b> — ou por nenhum.”</p></quote>" +
    "</section>" +
  "</body>";

const win = egui.openWindow("HTML rico — pipeline unificado (DOM no rts-dom)", 820, 680, 0);
// o DOM e do rts-dom (headless); o egui so LE e pinta.
const d = dom.parseHtml(HTML);

while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  egui.beginFrame(win);
  egui.render(win, d);
  egui.endFrame(win);
}

egui.close(win);
dom.free(d);
