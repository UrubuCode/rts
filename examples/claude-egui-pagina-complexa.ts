import egui from "rts:egui";
import dom from "rts:dom";

// PÁGINA WEB COMPLEXA — teste visual do motor HTML/egui (F0-F2). Exercita:
// box model (bg/padding/border/raio/margin), unidades (px/%/vw), estilo por tag +
// style="" inline, hierarquia de blocos, headings, listas, texto rico (b/i),
// width relativo resolvido contra o pai. Um "dashboard" estilizado.
//   target/release/rts.exe run examples/claude-egui-pagina-complexa.ts

// Slots de estilo (contrato com o Rust, style.rs).
const COLOR = 0, BG = 1, FONT = 2, PAD = 3, MARGIN = 4, BW = 5, BC = 6, RADIUS = 7, WIDTH = 8;
// Codificação de width (Dimension): faixa por unidade, valor × 1000.
const DR = 1000000000;
const pct = (p: number): number => 1 * DR + p * 1000;  // %
const vw = (v: number): number => 4 * DR + v * 1000;   // vw

// ── Layout das tags (display: 0=vertical 1=wrap 2=horizontal 3=grid) ──────────
dom.defineBlock("body", 0, 0, 0, 0);
dom.defineBlock("header", 0, 0, 0, 0);
dom.defineBlock("h1", 0, 30, 0, 4);
dom.defineBlock("h2", 0, 20, 0, 4);
dom.defineBlock("div", 0, 0, 0, 0);
dom.defineBlock("section", 0, 0, 0, 0);
dom.defineBlock("ul", 0, 0, 0, 0);
dom.defineBlock("li", 1, 0, 1, 0); // wrap + bullet
dom.defineBlock("p", 1, 0, 0, 0);  // parágrafo (wrap)
dom.defineBlock("footer", 0, 0, 0, 0);

// ── Estilo por TAG ────────────────────────────────────────────────────────────
// header: faixa escura no topo, largura 100% da viewport, padding generoso.
dom.defineStyle("header", BG, 0x0E1628FF);
dom.defineStyle("header", PAD, 18);
dom.defineStyle("header", RADIUS, 0);
dom.defineStyle("header", WIDTH, vw(100));

dom.defineStyle("h1", COLOR, 0x66CCFFFF);
dom.defineStyle("h1", FONT, 30);
dom.defineStyle("h2", COLOR, 0xAAD4FFFF);
dom.defineStyle("h2", FONT, 21);

// section: cartão de conteúdo, 90% da largura, fundo azul-escuro, borda, raio.
dom.defineStyle("section", BG, 0x16213AFF);
dom.defineStyle("section", PAD, 16);
dom.defineStyle("section", MARGIN, 10);
dom.defineStyle("section", BW, 1);
dom.defineStyle("section", BC, 0x335588FF);
dom.defineStyle("section", RADIUS, 12);
dom.defineStyle("section", WIDTH, pct(90));

dom.defineStyle("p", COLOR, 0xC8D2E0FF);
dom.defineStyle("p", FONT, 15);
dom.defineStyle("li", COLOR, 0xB0E0C0FF);
dom.defineStyle("li", FONT, 15);
dom.defineStyle("footer", COLOR, 0x8090A0FF);
dom.defineStyle("footer", FONT, 12);
dom.defineStyle("footer", PAD, 10);

const win = egui.openWindow("Dashboard — pagina complexa (HTML/egui F0-F2)", 760, 620, 0);

// ── A PÁGINA (HTML rico) ──────────────────────────────────────────────────────
const HTML =
  "<body>" +
    "<header>" +
      "<h1>RTS Dashboard</h1>" +
      "<p style=\"color:#7FB0D0; font-size:13\">Motor HTML proprio sobre egui — box model, unidades, estilo por slot opaco.</p>" +
    "</header>" +

    "<section>" +
      "<h2>Estatisticas</h2>" +
      "<div style=\"background-color:#1E2D4A; padding:12; border-radius:8; margin:6; width:280px\">" +
        "<p style=\"color:#66FF99; font-size:22\">1.719 testes</p>" +
        "<p style=\"color:#90A0B0; font-size:12\">suite TS verde</p>" +
      "</div>" +
      "<div style=\"background-color:#2D1E3A; padding:12; border-radius:8; margin:6; width:50%\">" +
        "<p style=\"color:#FF99CC; font-size:22\">34.6% paridade</p>" +
        "<p style=\"color:#90A0B0; font-size:12\">cross-runtime (honesta)</p>" +
      "</div>" +
    "</section>" +

    "<section>" +
      "<h2>Recursos do motor</h2>" +
      "<ul>" +
        "<li>Box model: <b>bg</b>, padding, borda, raio, margem</li>" +
        "<li>Unidades: <b>px</b>, <b>%</b>, em, rem, <b>vw</b>, vh, auto</li>" +
        "<li>Estilo: tag &lt; <i>style=\"\"</i> inline &lt; setStyleBatch por-no</li>" +
        "<li>DOM: parentNode, firstChild, classList, createTextNode</li>" +
      "</ul>" +
    "</section>" +

    "<section style=\"background-color:#1A2E1E; border-color:#3A6A4A; width:60%\">" +
      "<h2>Nota</h2>" +
      "<p>Esta <b>section</b> tem estilo de tag <i>sobreposto</i> por style=\"\" inline " +
      "(fundo verde-escuro, borda verde, largura 60% do pai). O <b>width%</b> resolve " +
      "tarde contra o content-box — aninha e cascateia como no browser.</p>" +
    "</section>" +

    "<footer>RTS — TypeScript-to-native + motor HTML/egui. Pagina renderizada 100%% via egui (zero painter absoluto).</footer>" +
  "</body>";

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
