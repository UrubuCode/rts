// Loader do site do RTS que LÊ os arquivos em runtime e os JUNTA:
//   site/index.html  → estrutura (sem CSS)
//   site/style.css   → todo o CSS (com o fundo de blocos animados)
// O CSS é injetado como <style> na cascade. Edite os arquivos e reabra o MESMO
// .exe — sem recompilar.
//
//   JIT:  target/release/rts.exe run examples/claude-rts-site-file.ts
//   AOT:  target/release/rts.exe compile --all-namespaces examples/claude-rts-site-file.ts dist/RTS-Site-File.exe
//   (rode o .exe da pasta que contém a subpasta site/, ex.: dentro de dist/)
import egui from "rts:egui";
import dom from "rts:dom";
import { fs, io } from "rts";

// Acha o HTML e o CSS relativos ao diretório de trabalho, tentando alguns locais.
function readFirst(a: string, b: string, c: string): string {
  if (fs.exists(a)) return fs.read_text(a);
  if (fs.exists(b)) return fs.read_text(b);
  if (fs.exists(c)) return fs.read_text(c);
  return "";
}

const htmlDoc = readFirst("site/index.html", "dist/site/index.html", "");
const css = readFirst("site/style.css", "dist/site/style.css", "");

let html = htmlDoc;
if (html.length === 0) {
  io.print("AVISO: site/index.html nao encontrado — usando fallback.");
  html = "<body style='background:#0b0d11;color:#e6e9ef;font-family:sans-serif'>"
    + "<div style='padding:60px;text-align:center'>"
    + "<h1 style='color:#f97316;font-size:40px'>RTS</h1>"
    + "<p style='color:#a4afc4'>Coloque a pasta site/ (index.html + style.css) ao lado do .exe.</p>"
    + "</div></body>";
} else {
  // injeta o CSS como <style> no topo — a cascade do motor o aplica ao documento.
  html = "<style>" + css + "</style>" + html;
}

io.print("html=" + htmlDoc.length + "B  css=" + css.length + "B  total=" + html.length + "B");

const d = dom.parseHtml(html);
const win = egui.openWindow("RTS — TypeScript compilado para nativo", 1100, 900, 0);
while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  egui.beginFrame(win);
  egui.render(win, d);
  egui.endFrame(win);
}
egui.close(win);
dom.free(d);
