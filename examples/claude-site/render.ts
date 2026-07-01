import egui from "rts:egui";
import dom from "rts:dom";
import fs from "rts:fs";

// Carrega um SITE REAL do disco e RENDERIZA na tela — HTML + CSS externo (<link>)
// + @import, tudo resolvido pela cascade do rts-dom; o egui só pinta.
//   target/release/rts.exe run examples/claude-site/render.ts

// 1) Lê o HTML do arquivo (como abrir file:// no browser).
const html = fs.read_text("examples/claude-site/index.html");

// 2) Parseia → DOM, carrega <link rel=stylesheet>/@import.
const doc = parseDocument(html);
const h = doc._dom;
loadResources(doc, "examples/claude-site/index.html");

// 3) Abre a janela e renderiza o DOM (com toda a cascade já aplicada).
const win = egui.openWindow("Site real carregado pelo RTS", 800, 640, 0);
while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  egui.beginFrame(win);
  egui.render(win, h);
  egui.endFrame(win);
}
egui.close(win);
