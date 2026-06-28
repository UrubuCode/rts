// Texto/fonte (#1749): text-align, line-height, text-transform, font-family.
//   target/release/rts.exe run examples/claude-css-text.ts
import egui from "rts:egui";
import dom from "rts:dom";

const html =
  "<style>" +
  "  body{background:#0f1420;color:#d8dee9;padding:20px;font-size:18px}" +
  "  h1{color:#88ccff;font-size:30px;text-align:center}" +
  "  .right{text-align:right;background:#1a2030;padding:10px}" +
  "  .center{text-align:center;background:#142a44;padding:10px}" +
  "  .upper{text-transform:uppercase;color:#7ee787}" +
  "  .spaced{line-height:2.2;background:#1a2030;padding:10px}" +
  "  .mono{font-family:monospace;color:#ffb454}" +
  "</style>" +
  "<h1>Título Centralizado</h1>" +
  "<div class='right'>Este texto está alinhado à direita</div>" +
  "<div class='center'>Este texto está centralizado</div>" +
  "<div class='upper'>maiúsculas automáticas</div>" +
  "<div class='spaced'>Linha com line-height 2.2 — bem espaçada do resto</div>" +
  "<div class='mono'>fonte monoespaçada: codigo()</div>";

const d = dom.parseHtml(html);
const win = egui.openWindow("RTS — Texto/Fonte (#1749)", 700, 480, 0);
while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  egui.beginFrame(win);
  egui.render(win, d);
  egui.endFrame(win);
}
egui.close(win);
dom.free(d);
