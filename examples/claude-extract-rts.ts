// EXTRATOR de layout do nosso DOM — para comparar com o Chrome número-a-número.
// Lê o dashboard.html, e para CADA seletor-chave dá: rect(x,y,w,h) + cor + bg +
// font-size + text-align + textContent. Formato igual ao claude-extract-chrome.
//   target/release/rts.exe run examples/claude-extract-rts.ts > rts_layout.txt
import dom from "rts:dom";
import { fs, io } from "rts";

const html = fs.read_text("examples/dashboard.html");
const d = dom.parseHtml(html);
const vw = 1080;

// um seletor por linha; pega o PRIMEIRO que casa (querySelector por subárvore da raiz).
function emit(sel: string): void {
  const root = dom.rootId(d);
  const n = dom.queryWithin(d, root, sel);
  if (n === -1) {
    io.print(sel + " | NAO_ACHADO");
    return;
  }
  // rect via boundingComponent (×1000): which 0=x 1=y 2=w 3=h
  const x = dom.boundingComponent(d, n, vw, 0) / 1000;
  const y = dom.boundingComponent(d, n, vw, 1) / 1000;
  const w = dom.boundingComponent(d, n, vw, 2) / 1000;
  const h = dom.boundingComponent(d, n, vw, 3) / 1000;
  const color = dom.computedProperty(d, n, "color");
  const bg = dom.computedProperty(d, n, "background-color");
  const fs2 = dom.computedProperty(d, n, "font-size");
  const ta = dom.computedProperty(d, n, "text-align");
  const txt = dom.getText(d, n);
  const txt40 = txt.length > 40 ? txt.substring(0, 40) : txt;
  io.print(sel + " | x=" + x + " y=" + y + " w=" + w + " h=" + h +
    " | color=" + color + " bg=" + bg + " fs=" + fs2 + " ta=" + ta + " | " + txt40);
}

emit(".nav");
emit(".nav .wordmark");
emit(".nav .cta");
emit(".hero");
emit(".hero h1");
emit(".hero .accent");
emit(".hero .lead");
emit(".hero .actions");
emit(".btn.primary");
emit(".stats");
emit(".stats .num");
emit(".features");
emit(".feature");
emit(".feature h3");
emit(".feature p");
emit(".trusted");
emit(".cta-box");
emit(".cta-box h2");
emit(".footer");
dom.free(d);
