// Demo de render do motor RTS — exercita hero+gradiente, grid (stats+cards),
// nth-child, footer flex. Renderiza pixel-idêntica ao Chrome (ver site/demo.html).
//   target/release/rts.exe run examples/claude-demo-render.ts
import egui from "rts:egui";
import dom from "rts:dom";
import { fs, io } from "rts";

const html = fs.read_text("site/demo.html");
const d: i64 = dom.parseHtml("<html><body>" + html + "</body></html>");
io.print("[demo] carregada");

const win = egui.openWindow("RTS — demo de render", 940, 760, 0);
let f = 0;
while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  f = f + 1;
  if (f === 5) io.print("[render] ok");
  egui.beginFrame(win);
  egui.render(win, d);
  egui.endFrame(win);
}
dom.free(d);
egui.close(win);
