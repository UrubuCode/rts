// Visualizador de HTML na janela, pelo motor NOVO — o DOM do `rts-dom` na tela.
//
//   cargo run --release -p rts-host --features ui --example ui_fixture -- \
//       examples/claude-dom-viewer.ts
//
// O caminho da página vai na constante abaixo porque o `ui_fixture` já usa o
// primeiro argumento para o próprio `.ts`. Trocar a página é editar a linha e
// rodar de novo — sem recompilar nada, que é a razão de o `ui_fixture` existir.
//
// O que se vê aqui é o mesmo caminho que as métricas medem: `egui.html` parseia
// para a árvore retida do `rts-dom` (com cache por hash, então o loop não
// re-parseia), o layout calcula a geometria e o egui só pinta a display list.

import {
  openWindow, pump, isOpen, close, beginFrame, endFrame,
  html, drawText, winWidth, winHeight,
} from "rts:egui";
import { readFileSync } from "node:fs";

const PAGINA = "examples/claude-ai-site.html";

const fonte = readFileSync(PAGINA, "utf8");
console.log("página:", PAGINA, "|", fonte.length, "bytes");

const win = openWindow("rts-dom — " + PAGINA, 1100, 780, 0);
if (win <= 0) {
  console.log("não abriu a janela");
} else {
  let frames = 0;
  while (isOpen(win)) {
    pump(win);
    beginFrame(win);
    html(win, fonte);
    // Rodapé com o tamanho real da área: confirma que a página está sendo
    // disposta contra a janela, e não contra um viewport fixo.
    drawText(win, "rts-dom · " + winWidth(win) + "x" + winHeight(win) + " · frame " + frames, 0);
    endFrame(win);
    frames = frames + 1;
  }
  close(win);
  console.log("fechou depois de", frames, "frames");
}
