import egui from "rts:egui";
import fs from "rts:fs";
import env from "rts:env";

// VIEWER genérico do motor de render: abre qualquer página HTML local numa
// janela, com CSS externo (<link rel=stylesheet> + @import relativos) via
// loadResources. Uso:
//
//   rts run examples/view.ts <caminho/para/index.html>
//
// Limites atuais (issue #1793): recursos http(s) não carregam (use assets
// locais), <img> não renderiza, o JS da página não executa.
const n = env.args_count();
const path = n > 3 ? env.arg_at(3) : "";
if (path.length === 0) {
  console.log("uso: rts run examples/view.ts <caminho/para/index.html>");
} else {
  const html = fs.read_text(path);
  if (html.length === 0) {
    console.log("nao consegui ler: " + path);
  } else {
    console.log("HTML: " + html.length + " bytes");
    const doc = parseDocument(html);
    const loaded = loadResources(doc, path);
    console.log("recursos externos carregados: " + loaded);
    const win = egui.openWindow("RTS — " + path, 1100, 750, 0);
    while (egui.isOpen(win) !== 0) {
      if (egui.pump(win) !== 0) break;
      egui.beginFrame(win);
      egui.render(win, doc._dom);
      egui.endFrame(win);
    }
    egui.close(win);
  }
}
