// A Wikipédia real na nossa janela: 2 MB de HTML e 257 KB de CSS do MediaWiki,
// pintados pelo nosso motor, sem browser nenhum no caminho.
import { openWindow, pump, isOpen, close, beginFrame, endFrame, html, drawText } from "rts:egui";
import { readFileSync } from "node:fs";
const fonte = "<style>" + (readFileSync("pagina.css", "utf8") as string) + "</style>" +
              (readFileSync("pagina.html", "utf8") as string);
console.log("documento + folhas:", fonte.length, "bytes");
const win = openWindow("rts-dom — pt.wikipedia.org/wiki/Brasil", 1280, 820, 0);
let frames = 0;
while (isOpen(win)) {
  pump(win); beginFrame(win); html(win, fonte);
  drawText(win, "rts-dom · pt.wikipedia.org/wiki/Brasil · " + fonte.length + " bytes · frame " + frames, 0);
  endFrame(win); frames = frames + 1;
}
close(win);
console.log("fechou depois de", frames, "frames");
