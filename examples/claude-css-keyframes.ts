// @keyframes (#1776 fase 2) — animação que roda SOZINHA no tempo, sem gatilho.
// O DOM é dono do loop (advance por frame); o egui só passa o tempo e pinta.
//   target/release/rts.exe run examples/claude-css-keyframes.ts
import egui from "rts:egui";
import dom from "rts:dom";

const d = dom.parseHtml(
  "<style>" +
  "  body{background:#0f1420;padding:30px}" +
  "  @keyframes pulse {" +
  "    0%   { background:#2563eb; width:100px; border-radius:8px }" +
  "    50%  { background:#ff6b6b; width:280px; border-radius:50px }" +
  "    100% { background:#2563eb; width:100px; border-radius:8px }" +
  "  }" +
  "  @keyframes grow {" +
  "    from { background:#7ee787; height:40px }" +
  "    to   { background:#ffb454; height:140px }" +
  "  }" +
  "  #pulse{height:100px;animation:pulse 2s ease-in-out infinite}" +
  "  #grow{width:120px;margin:20px;animation:grow 1.5s ease infinite alternate}" +
  "</style>" +
  "<div id='pulse'>pulse infinito</div>" +
  "<div id='grow'>grow</div>");

const win = egui.openWindow("RTS — @keyframes (animação no tempo, DOM dono do loop)", 700, 420, 0);
while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  egui.beginFrame(win);
  egui.render(win, d);  // o egui chama advance(now) interno; as @keyframes rodam sozinhas
  egui.endFrame(win);
}
egui.close(win);
dom.free(d);
