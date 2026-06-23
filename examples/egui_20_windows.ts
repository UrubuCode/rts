// Stress de RAM: abre 20 janelas egui (Device GPU COMPARTILHADO — só Surface +
// Renderer por janela) e renderiza um rótulo em cada uma todo frame. Use pra
// medir o uso TOTAL de memória com várias janelas vivas:
//
//   target/release/rts.exe run examples/egui_20_windows.ts
//
// Feche as janelas (X) pra encerrar — o loop sai quando todas estão fechadas.
import egui from "rts:egui";

const COUNT = 20;
const handles: number[] = [];

let i = 0;
while (i < COUNT) {
  const h = egui.openWindow("janela " + i, 300, 200, 0);
  handles.push(h);
  i = i + 1;
}

// Loop principal: pumpa + renderiza cada janela ainda aberta; sai quando todas
// fecharam.
let anyOpen = true;
while (anyOpen) {
  anyOpen = false;
  let j = 0;
  while (j < COUNT) {
    const h = handles[j];
    if (egui.isOpen(h) !== 0) {
      anyOpen = true;
      egui.pump(h);
      egui.beginFrame(h);
      egui.html(h, "<h2>Janela " + j + "</h2><p>Device GPU <b>compartilhado</b>. Feche pra sair.</p>");
      egui.endFrame(h);
    }
    j = j + 1;
  }
}

// Fecha tudo (idempotente — janelas já fechadas pelo X só removem o handle).
i = 0;
while (i < COUNT) {
  egui.close(handles[i]);
  i = i + 1;
}
