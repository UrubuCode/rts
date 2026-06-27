// Carrega um arquivo .html DO DISCO e renderiza — o que Node/Bun não fazem sem
// jsdom. Pipeline 100% nativo: fs.read_text → dom.parseHtml → egui.render.
// O HTML pode ser COMPLETO (<head>/<title>/<meta>/<style>) — só o <body> pinta.
//   target/release/rts.exe run examples/claude-load-html-file.ts
import egui from "rts:egui";
import dom from "rts:dom";
import { fs, io } from "rts";

// ── Defaults de display das tags HTML (o que o navegador tem embutido). ──────────
// display: 0=block(vertical) 1=wrap(inline-block) 2=horizontal(flex-row).
// (No futuro a fachada do rts-dom registra isto sozinha; por ora, explícito.)
function block(tag: string) { dom.defineBlock(tag, 0, 0, 0, 6); }
block("html"); block("body"); block("header"); block("footer");
block("section"); block("div"); block("h1"); block("h2"); block("h3");
dom.defineBlock("p", 1, 0, 0, 0);     // <p> flui (wrap) — texto inline
dom.defineBlock("row", 2, 0, 0, 8);   // <row> = flex-row (lado a lado)
dom.defineBlock("tags", 1, 0, 0, 8);  // <tags> = wrap (flui e quebra linha)

// ── 1. LÊ o arquivo .html do disco (string). ─────────────────────────────────────
const html = fs.read_text("examples/pagina.html");
io.print("arquivo lido: " + html.length + " bytes");

// ── 2. PARSEIA para um DOM com estilo+layout (tudo no rts-dom). ──────────────────
const d = dom.parseHtml(html);
io.print("DOM montado, root = " + dom.querySelector(d, "body"));

// ── 3. RENDERIZA numa janela (egui só lê a DisplayList do DOM e pinta). ──────────
const win = egui.openWindow("RTS — arquivo .html carregado do disco", 820, 640, 0);
while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  egui.beginFrame(win);
  egui.render(win, d);
  egui.endFrame(win);
}
egui.close(win);
dom.free(d);
