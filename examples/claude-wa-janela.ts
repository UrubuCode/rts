// O WhatsApp Web real na nossa janela, do que a nossa pilha baixou.
import { openWindow, pump, isOpen, close, beginFrame, endFrame, html, drawText } from "rts:egui";
import { readFileSync } from "node:fs";
const fonte = "<style>" + (readFileSync("wa-app.css", "utf8") as string) + "</style>" +
              (readFileSync("wa-app.html", "utf8") as string);
console.log("documento + folhas:", fonte.length, "bytes");
const win = openWindow("rts-dom — web.whatsapp.com", 1100, 780, 0);
let frames = 0;
while (isOpen(win)) {
  pump(win); beginFrame(win); html(win, fonte);
  drawText(win, "rts-dom · web.whatsapp.com · " + fonte.length + " bytes · frame " + frames, 0);
  endFrame(win); frames = frames + 1;
}
close(win);
console.log("fechou depois de", frames, "frames");
