// CANVAS no nosso DOM: o programa pinta os pixels, o DOM os carrega, o egui os
// desenha. É o pipeline inteiro — documento → display list → tela — com o
// conteúdo vindo de código em vez de de uma tag.
//
//   cargo run --release -p rts-host --features ui --example ui_fixture -- \
//       examples/claude-canvas.ts

import {
  openWindow, pump, isOpen, close, beginFrame, endFrame, render, drawText,
} from "rts:egui";
import { parseHtml, querySelector, setPixels, setText } from "rts:dom";

const LADO = 260;

const doc = parseHtml(
  "<html><head><style>" +
  "body{background:#faf7f2}" +
  "h1{font-size:26px;color:#1d1f1f}" +
  ".cartao{background:#ffffff;padding:24px;margin:24px}" +
  "p{color:#555555}" +
  "</style></head><body><div class='cartao'>" +
  "<h1>Canvas pintado por código</h1>" +
  "<p id='legenda'>os pixels abaixo saíram de um laço TypeScript</p>" +
  "<canvas id='tela' width='" + LADO + "' height='" + LADO + "'></canvas>" +
  "</div></body></html>"
);

const tela = querySelector(doc, "#tela");
const legenda = querySelector(doc, "#legenda");

// RGBA8, `lado*lado*4` bytes. Um Buffer é o que o `setPixels` lê — o mesmo
// caminho que o `<img>` do mini-browser usa depois de decodificar um PNG.
const pixels = Buffer.alloc(LADO * LADO * 4);

// Um padrão xadrez com um degradê: mostra que cada pixel veio do laço, e não de
// um retângulo que o layout desenhou.
function pintar(fase: number): void {
  let y = 0;
  while (y < LADO) {
    let x = 0;
    while (x < LADO) {
      const i = (y * LADO + x) * 4;
      const quadro = (((x + fase) / 20) | 0) + (((y + fase) / 20) | 0);
      const claro = (quadro % 2) === 0;
      pixels[i] = claro ? 255 - ((x * 255 / LADO) | 0) : 20;
      pixels[i + 1] = claro ? 200 - ((y * 120 / LADO) | 0) : 200 - ((x * 60 / LADO) | 0);
      pixels[i + 2] = claro ? 90 : 120;
      pixels[i + 3] = 255;
      x = x + 1;
    }
    y = y + 1;
  }
}

const win = openWindow("rts-dom — canvas", 900, 700, 0);
if (win <= 0) {
  console.log("não abriu a janela");
} else {
  let frames = 0;
  while (isOpen(win)) {
    pump(win);
    // repinta a cada 6 frames: prova que o caminho pixels→DOM→egui é vivo, e
    // não uma textura carregada uma vez.
    if (frames % 6 === 0) {
      pintar((frames / 6) | 0);
      // largura e altura empacotadas num inteiro (16 bits cada) — a fronteira
      // de um nativo tem quatro argumentos, e três já foram.
      setPixels(doc, tela, pixels, (LADO << 16) | LADO);
      setText(doc, legenda, "quadro " + ((frames / 6) | 0) + " — " + (LADO * LADO) + " pixels por laço TypeScript");
    }
    beginFrame(win);
    render(win, doc);
    drawText(win, "rts-dom · canvas · frame " + frames, 0);
    endFrame(win);
    frames = frames + 1;
  }
  close(win);
  console.log("fechou depois de", frames, "frames");
}
