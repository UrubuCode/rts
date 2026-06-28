// Renderiza examples/dashboard.html (landing SaaS profissional) no motor CSS nativo.
//   target/release/rts.exe run examples/claude-dashboard.ts
import egui from "rts:egui";
import dom from "rts:dom";
import { fs, io } from "rts";

const html = fs.read_text("examples/dashboard.html");
io.print("dashboard.html lido: " + html.length + " bytes");
const d = dom.parseHtml(html);

const win = egui.openWindow("RTS — Nebula (landing SaaS, motor CSS nativo)", 1100, 900, 0);
while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  egui.beginFrame(win);
  egui.render(win, d);
  egui.endFrame(win);
}
egui.close(win);
dom.free(d);
