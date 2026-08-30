// O WhatsApp Web real, com o JS DA PÁGINA a correr, na nossa janela.
//
// A diferença para o `claude-wa-janela.ts` ao lado é `render(win, doc)` em vez
// de `html(win, fonte)`: aquele parseia o texto a cada frame e pinta, este pinta
// a ÁRVORE — que é a única forma de se ver o que os `<script>` fizeram, porque
// o que eles mudam é o documento e não o texto que o gerou.
import { openWindow, pump, isOpen, close, beginFrame, endFrame, render, drawText } from "rts:egui";
import { readFileSync } from "node:fs";

const fonte = "<style>" + (readFileSync("wa-app.css", "utf8") as string) + "</style>" +
              (readFileSync("wa-app.html", "utf8") as string);
console.log("documento + folhas:", fonte.length, "bytes");

const doc = parseDocument(fonte);
const correram = runScripts(doc);
console.log("scripts que correram:", correram);

const win = openWindow("rts-dom — web.whatsapp.com (JS a correr)", 1280, 860, 0);
let frames = 0;
while (isOpen(win)) {
  pump(win);
  beginFrame(win);
  render(win, doc._dom);
  drawText(win, "rts-dom · web.whatsapp.com · " + correram + " scripts · frame " + frames, 0);
  endFrame(win);
  frames = frames + 1;
}
close(win);
console.log("fechou depois de", frames, "frames");
