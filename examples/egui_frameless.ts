// Janela SEM header do SO (frameless) + fundo TRANSPARENTE — o app desenha o
// próprio cabeçalho/conteúdo em HTML (visão "HTML como frontend de GUI"). Use
// render:"glow" p/ leveza. Aqui via primitivo cru (config bits): 16=transparent,
// 32=sem-decorations, 8=glow → 56.
import egui from "rts:egui";
const page =
  "<h1>App sem header</h1>" +
  "<p>Janela <b>transparente</b> e <b>frameless</b>. O cabeçalho/botões seriam HTML.</p>";
const h = egui.openWindow("frameless", 420, 300, 56);
let f = 0;
while (egui.isOpen(h) !== 0 && f < 5) {
  egui.pump(h);
  egui.beginFrame(h);
  egui.html(h, page);
  if (f === 3) egui.snapshot(h, "egui_frameless.ppm");
  egui.endFrame(h);
  f = f + 1;
}
egui.close(h);
