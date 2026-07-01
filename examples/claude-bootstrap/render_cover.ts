import egui from "rts:egui";
import dom from "rts:dom";
import fs from "rts:fs";

const path = "examples/bootstrap-5.3.8-examples/cover/index.html";
const html = fs.read_text(path);
console.log("HTML: " + html.length + " bytes");

const doc = parseDocument(html);
const h = doc._dom;
const n = loadResources(doc, path);
console.log("recursos externos carregados: " + n);

const win = egui.openWindow("Bootstrap Cover — renderizado pelo RTS", 1000, 700, 0);
while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  egui.beginFrame(win);
  egui.render(win, h);
  egui.endFrame(win);
}
egui.close(win);
