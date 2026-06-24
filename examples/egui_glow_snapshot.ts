// Teste visual HEADLESS do backend glow: abre uma janela OpenGL, renderiza HTML
// com texto/cabeçalhos, captura o frame num PPM e FECHA. O checker
// (tests/check_snapshot.sh ou inspeção manual) confirma que o PPM não está em
// branco — i.e. que o texto realmente pintou (não é só o fundo escuro).
//
//   target/release/rts.exe run examples/egui_glow_snapshot.ts
//   -> grava egui_glow_snapshot.ppm no cwd
import egui from "rts:egui";

const page =
  "<h1>SNAPSHOT OK</h1>" +
  "<p>Texto <b>negrito</b> e <i>italico</i> renderizados via <b>OpenGL</b> (glow).</p>" +
  "<h2>Linha 2</h2>" +
  "<p>Se este PPM tiver pixels claros, o texto pintou de verdade.</p>";

const h = egui.openWindow("glow snapshot", 480, 320, 8); // bit3 = glow

// Pinta alguns frames pra estabilizar (fontes/atlas sobem no 1º), agenda o
// snapshot e captura no endFrame seguinte, depois fecha.
let frame = 0;
while (frame < 4) {
  egui.pump(h);
  egui.beginFrame(h);
  egui.html(h, page);
  if (frame === 2) {
    egui.snapshot(h, "egui_glow_snapshot.ppm");
  }
  egui.endFrame(h);
  frame = frame + 1;
}
egui.close(h);
