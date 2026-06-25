import egui from "rts:egui";
import input from "rts:input";

// Teclado completo + modificadores (input fase 1). Mostra a tecla pressionada,
// os modificadores ativos, e detecta um atalho (Ctrl+S). Codigos neutros: 100-125
// A-Z, 130-139 0-9, 140-151 F1-F12, 1-15 edicao/navegacao.
//   target/release/rts.exe run examples/claude-keyboard.ts

const app = createAppAt("Teclado + modificadores", 480, 360, 2150, 420);

let lastKey = "-";
let shortcuts = 0;

while (app.running()) {
  if (!app.beginFrame()) break;
  app.fillRect(0, 0, 480, 360, 0x12161CFF);
  app.text(20, 16, "Pressione teclas (letras, F-keys, Ctrl/Shift...)", 0xB0B8C0FF, 15);

  // detecta letras A-Z
  let i = 0;
  while (i < 26) {
    if (input.keyPressed(app._win, 100 + i) !== 0) {
      // converte indice em letra: 'A'=65
      lastKey = "letra " + i; // (sem String.fromCharCode no PoC; mostra o indice)
    }
    i = i + 1;
  }
  // algumas teclas nomeadas
  if (input.keyPressed(app._win, 1) !== 0) lastKey = "Enter";
  if (input.keyPressed(app._win, 3) !== 0) lastKey = "Space";
  if (input.keyPressed(app._win, 9) !== 0) lastKey = "Tab";
  if (input.keyPressed(app._win, 140) !== 0) lastKey = "F1";
  if (input.keyPressed(app._win, 5) !== 0) lastKey = "Seta cima";
  if (input.keyPressed(app._win, 6) !== 0) lastKey = "Seta baixo";

  // modificadores ativos
  const ctrl = input.modCtrl(app._win);
  const shift = input.modShift(app._win);
  const alt = input.modAlt(app._win);

  // atalho: Ctrl + S (S = 100 + 18)
  if (ctrl !== 0 && input.keyPressed(app._win, 100 + 18) !== 0) {
    shortcuts = shortcuts + 1;
  }

  app.box(20, 50, 440, 70, 0x1A2230FF, 1, 0x33445566 & 0xFFFFFFFF, 8);
  app.text(36, 64, "Ultima tecla: " + lastKey, 0xFFFFFFFF, 18);
  app.text(36, 92, "Ctrl=" + ctrl + "  Shift=" + shift + "  Alt=" + alt, 0x99CCFFFF, 15);

  app.box(20, 140, 440, 60, 0x14301EFF, 1, 0x44DD6666 & 0xFFFFFFFF, 8);
  app.text(36, 158, "Atalho Ctrl+S detectado: " + shortcuts + " vezes", 0xAAFFCCFF, 16);

  // segurar tecla (keyDown) — barra que enche enquanto a seta direita esta down
  app.text(20, 220, "Segure a Seta Direita:", 0xC0C8D0FF, 14);
  let held = 0;
  if (input.keyDown(app._win, 8) !== 0) held = 1;
  let barColor = 0x333A44FF;
  if (held !== 0) barColor = 0x33CC88FF;
  app.box(20, 244, 440, 24, barColor, 0, 0, 6);

  app.text(20, 320, "keyPressed/keyDown/keyReleased + modCtrl/Shift/Alt/Cmd", 0x808890FF, 12);
  app.endFrame();
}

app.close();
