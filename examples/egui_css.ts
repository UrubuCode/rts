// FASE 0 do engine CSS nativo: estilo inline `style="..."` (cor/tamanho/peso).
import egui from "rts:egui";
const page =
  "<h1 style=\"color:#ff5555\">Titulo vermelho</h1>" +
  "<p style=\"color:#55aaff; font-size:20px\">Paragrafo azul grande.</p>" +
  "<p>Normal com <b style=\"color:orange\">laranja negrito</b> e " +
  "<span style=\"color:#22dd22; font-style:italic\">verde italico</span> inline.</p>";
const h = egui.openWindow("css P0", 460, 320, 8); // glow
let f = 0;
while (egui.isOpen(h) !== 0 && f < 5) {
  egui.pump(h); egui.beginFrame(h); egui.html(h, page);
  if (f === 3) egui.snapshot(h, "egui_css.ppm");
  egui.endFrame(h); f = f + 1;
}
egui.close(h);
