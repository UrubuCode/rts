// Janela egui mostrando o FLEXBOX do motor DOM: toolbar (justify-content:
// space-between + align-items:center) e cards (gap + flex). Layout 100% no rts-dom,
// egui só pinta a DisplayList.
//   target/release/rts.exe run examples/claude-flex-window.ts
import egui from "rts:egui";
import dom from "rts:dom";

const html =
  "<style>" +
  "  body{background:#0f1420;padding:20px}" +
  "  h1{color:#88ccff;font-size:28px}" +
  "  bar{display:flex;justify-content:space-between;align-items:center;height:56px;background:#1a2030;border:1px solid #2e3a52;border-radius:8px;padding:12px}" +
  "  .btn{width:130px;height:38px;background:#2563eb;border-radius:6px}" +
  "  cards{display:flex;gap:20px}" +
  "  .card{width:30%;height:90px;background:#142a44;border:1px solid #3a6ea5;border-radius:10px;box-sizing:border-box;padding:14px}" +
  "</style>" +
  "<h1>Flexbox no motor DOM</h1>" +
  "<bar><div class='btn'>Voltar</div><div class='btn'>Salvar</div><div class='btn'>Sair</div></bar>" +
  "<cards><div class='card'>A</div><div class='card'>B</div><div class='card'>C</div></cards>";

const d = dom.parseHtml(html);
const win = egui.openWindow("RTS — Flexbox (gap + justify + align)", 900, 400, 0);
while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  egui.beginFrame(win);
  egui.render(win, d);
  egui.endFrame(win);
}
egui.close(win);
dom.free(d);
