// transition CSS (#1776) — o DOM é dono do LOOP de animação; o egui só passa o tempo
// e pinta (continua burro). O JS muda o estilo; a transition interpola suave.
//   target/release/rts.exe run examples/claude-css-transition.ts
import egui from "rts:egui";
import dom from "rts:dom";
import { time } from "rts";

const d = dom.parseHtml(
  "<style>" +
  "  body{background:#0f1420;padding:30px}" +
  "  #box{width:120px;height:120px;background:#2563eb;border-radius:10px;" +
  "       transition:1s ease-in-out}" +
  "</style>" +
  "<div id='box'>anima</div>");

const box = dom.querySelector(d, "#box");
const win = egui.openWindow("RTS — transition (DOM dono do loop, egui burro)", 600, 400, 0);

let toggled = 0;
let lastSwap = time.now_ms();

while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  // a cada 1.5s, alterna o estilo do box — a transition suaviza a mudança.
  const now = time.now_ms();
  if (now - lastSwap > 1500) {
    lastSwap = now;
    if (toggled === 0) {
      dom.setStyleProperty(d, box, "background", "#ff6b6b");
      dom.setStyleProperty(d, box, "width", "300px");
      dom.setStyleProperty(d, box, "border-radius", "60px");
      toggled = 1;
    } else {
      dom.setStyleProperty(d, box, "background", "#2563eb");
      dom.setStyleProperty(d, box, "width", "120px");
      dom.setStyleProperty(d, box, "border-radius", "10px");
      toggled = 0;
    }
  }
  egui.beginFrame(win);
  egui.render(win, d);  // o egui chama advance(now) internamente e pinta o interpolado
  egui.endFrame(win);
}
egui.close(win);
dom.free(d);
