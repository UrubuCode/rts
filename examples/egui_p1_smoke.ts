// P1 — PoC do gate de risco do rts:egui.
// Abre uma janela wgpu, roda o loop dirigido pelo TS (while + pump + begin/end
// frame), mostra um label + botão que conta cliques + um slider.
//
// NÃO é um teste de suíte (abre janela real). Rodar manualmente:
//   target/release/rts.exe run examples/egui_p1_smoke.ts
//
// Valida: openWindow não dá panic com tokio inicializado; o while-loop TS chama
// os primitivos sem travar; a fila de comandos desenha label/button/slider; o
// present() do wgpu funciona. Ver docs/specs/egui-ui-crate-design.md (P1).

import egui from "rts:egui";

// backend: 0 = wgpu (default/primário), 1 = glow (fallback).
const WGPU = 0;

const win = egui.openWindow("RTS egui — P1 smoke", 340, 600, WGPU);

let clicks = 0;
let volume = 0.5;

while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break; // janela fechada

  egui.beginFrame(win);

  egui.label(win, "RTS egui PoC — loop dirigido pelo TS");
  egui.label(win, "Cliques: " + clicks);

  if (egui.button(win, "Clique aqui") !== 0) {
    clicks = clicks + 1;
  }

  volume = egui.slider(win, volume, 0.0, 1.0);
  egui.label(win, "Volume: " + volume);

  egui.endFrame(win);
}

egui.close(win);
