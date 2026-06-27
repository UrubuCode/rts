// Carrega um arquivo .html DO DISCO e renderiza — o que Node/Bun não fazem sem
// jsdom. Pipeline 100% nativo: fs.read_text → dom.parseHtml → egui.render.
// O HTML pode ser COMPLETO (<head>/<title>/<meta>/<style>) — só o <body> pinta.
//   target/release/rts.exe run examples/claude-load-html-file.ts
import egui from "rts:egui";
import dom from "rts:dom";
import { fs, io } from "rts";

// SEM defineBlock! As tags HTML (div/p/section/h1…) já são block embutidas no
// motor (UA-stylesheet), e o LAYOUT (display:flex/block/none) vem do CSS no
// <style>. O HTML+CSS é autônomo — exatamente como no navegador.

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
