// A menor página possível na janela: se isto não aparece, o problema não é a
// página real — é o caminho de pintura.
import { openWindow, pump, isOpen, close, beginFrame, endFrame, html, drawText } from "rts:egui";
const fonte = "<html><body style='background:#ffdd55'>" +
  "<h1 style='color:#aa0000'>RTS DOM</h1>" +
  "<p style='color:#003366'>Se você vê este texto, o caminho de pintura funciona.</p>" +
  "<div style='background:#2277cc;width:400px;height:120px'></div>" +
  "</body></html>";
const win = openWindow("rts-dom — teste mínimo", 900, 600, 0);
let frames = 0;
while (isOpen(win)) {
  pump(win);
  beginFrame(win);
  html(win, fonte);
  drawText(win, "frame " + frames, 0);
  endFrame(win);
  frames = frames + 1;
}
close(win);
console.log("frames:", frames);
