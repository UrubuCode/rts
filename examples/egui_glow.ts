// Janela egui com backend OpenGL (glow) — MUITO mais leve em RAM que o wgpu/DX12
// default (dezenas de MB vs ~224 MB), pois o GL não reserva heap de driver GPU.
//
//   target/release/rts.exe run examples/egui_glow.ts
//
// Basta `render: "glow"` (ou "opengl") nas opções. O resto da API é idêntico ao
// backend wgpu — mesmo HTML, mesmos widgets.
import egui from "rts:egui";

const page =
  "<h1>RTS egui — OpenGL</h1>" +
  "<p>Esta janela usa o backend <b>glow</b> (OpenGL), bem mais <i>leve</i> em RAM.</p>" +
  "<h2>Por que</h2>" +
  "<p>O wgpu/DX12 reserva ~224 MB de heap de driver mesmo p/ uma UI 2D. O OpenGL nao.</p>";

const h = egui.openWindow("egui glow", 460, 320, 8); // bit3 = glow
while (egui.isOpen(h) !== 0) {
  if (egui.pump(h) !== 0) break;
  egui.beginFrame(h);
  egui.html(h, page);
  egui.endFrame(h);
}
egui.close(h);
