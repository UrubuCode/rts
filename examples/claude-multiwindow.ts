// MULTI-WINDOW — um programa com VÁRIAS janelas (como um navegador tem N abas/
// janelas). Cada janela é um App independente (handle próprio, UiCtx próprio); um
// loop só gerencia as duas. Prova que multi-window já é suportado.
//   target/release/rts.exe run examples/claude-multiwindow.ts

const a = createApp("Janela A", 360, 240);
a.moveTo(2100, 300);
const b = createApp("Janela B", 360, 240);
b.moveTo(2550, 300);

let countA = 0;
let countB = 0;

// um loop só, gerenciando AS DUAS janelas. roda enquanto QUALQUER uma estiver
// aberta; cada janela desenha seu próprio conteúdo.
while (a.running() || b.running()) {
  // janela A
  if (a.running()) {
    if (a.beginFrame()) {
      a.fillRect(0, 0, 360, 240, 0x101820FF);
      a.text(20, 20, "Janela A", 0x66CCFFFF, 22);
      a.text(20, 60, "Cliques A: " + countA, 0xFFFFFFFF, 16);
      if (a.button(20, 90, 160, 40, "Clique A")) countA = countA + 1;
      a.text(20, 200, "fecha so a A; a B continua", 0x808890FF, 12);
      a.endFrame();
    }
  }
  // janela B (independente)
  if (b.running()) {
    if (b.beginFrame()) {
      b.fillRect(0, 0, 360, 240, 0x201018FF);
      b.text(20, 20, "Janela B", 0xFF99AAFF, 22);
      b.text(20, 60, "Cliques B: " + countB, 0xFFFFFFFF, 16);
      if (b.button(20, 90, 160, 40, "Clique B")) countB = countB + 1;
      b.text(20, 200, "duas janelas, um programa", 0x808890FF, 12);
      b.endFrame();
    }
  }
}

a.close();
b.close();
