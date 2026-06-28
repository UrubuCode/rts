// Flexbox: gap + justify-content + align-items, layout no DOM (egui só pinta).
//   target/release/rts.exe run examples/claude-dom-flexbox.ts
import dom from "rts:dom";
import { io } from "rts";

dom.defineBlock("toolbar", 2, 0, 0, 0); // display:flex via defineBlock OU CSS
dom.defineBlock("div", 0, 0, 0, 0);

const d = dom.parseHtml(
  "<style>" +
  "  bar{display:flex;justify-content:space-between;align-items:center;gap:16px;height:60px}" +
  "  .btn{width:120px;height:40px;background:#2563eb}" +
  "</style>" +
  "<bar>" +
  "  <div class='btn'>Voltar</div>" +
  "  <div class='btn'>Salvar</div>" +
  "  <div class='btn'>Sair</div>" +
  "</bar>");

io.print("toolbar flex (space-between + center + gap):");
dom.dumpLayout(d, 800);
