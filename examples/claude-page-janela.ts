// Uma página REAL da web na nossa janela — baixada pela nossa pilha (TLS 1.3,
// HTTP à mão, `chunked` desmontado), com as folhas externas buscadas e
// embutidas, parseada e disposta pelo `rts-dom` e pintada pelo backend.
//
// A Wikipédia é o caso que prova o pipeline inteiro porque é servida PRONTA: o
// HTML que chega já tem o artigo e as classes que a folha estiliza. Um app que
// monta o DOM no cliente (o WhatsApp Web) mostra só o shell aqui, e a diferença
// é o JavaScript da página — está medida em `examples/claude-wa-dois.ts`.
import { openWindow, pump, isOpen, close, beginFrame, endFrame, html, drawText } from "rts:egui";
import { readFileSync } from "node:fs";

const fonte = "<style>" + (readFileSync("pagina.css", "utf8") as string) + "</style>" +
              (readFileSync("pagina.html", "utf8") as string);
console.log("documento + folhas:", fonte.length, "bytes");
const win = openWindow("rts-dom — pt.wikipedia.org/wiki/Brasil", 1100, 780, 0);
let frames = 0;
while (isOpen(win)) {
  pump(win);
  beginFrame(win);
  html(win, fonte);
  drawText(win, "rts-dom · pt.wikipedia.org · " + fonte.length + " bytes · frame " + frames, 0);
  endFrame(win);
  frames = frames + 1;
}
close(win);
console.log("fechou depois de", frames, "frames");
