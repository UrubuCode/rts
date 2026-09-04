import egui from "rts:egui";
import { readFileSync } from "node:fs";

// VIEWER genérico do motor de render: abre qualquer página HTML local numa
// janela, com CSS externo (<link rel=stylesheet> + @import relativos) via
// loadResources. Uso:
//
//   rts run examples/view.ts <caminho/para/index.html>
//
// Limites atuais (crates/rts-dom/PLAN.md §0): recursos http(s) não carregam
// (use assets locais; <img> local e `data:` já renderizam), o JS da página não
// executa aqui. `rts:fs`/`rts:env` eram do motor antigo: o ficheiro lê-se por
// `node:fs` e o caminho vem de `process.argv`.
let path = "";
for (const a of process.argv) { if (a.length > 5 && a.substring(a.length - 5) === ".html") path = a; }
if (path.length === 0) {
  console.log("uso: rts run examples/view.ts <caminho/para/index.html>");
} else {
  const html = readFileSync(path, "utf8") as string;
  if (html.length === 0) {
    console.log("nao consegui ler: " + path);
  } else {
    console.log("HTML: " + html.length + " bytes");
    const doc = parseDocument(html);
    const loaded = loadResources(doc, path);
    console.log("recursos externos carregados: " + loaded);
    const win = egui.openWindow("RTS — " + path, 1100, 750, 0);
    while (egui.isOpen(win)) {
      if (!egui.pump(win)) break;
      egui.beginFrame(win);
      egui.render(win, doc._dom);
      egui.endFrame(win);
    }
    egui.close(win);
  }
}
