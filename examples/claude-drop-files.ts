import egui from "rts:egui";
import input from "rts:input";

// Fumaça de arquivos soltos/pairando (drag-and-drop do SO — Explorer/Finder).
// Não dá para simular o arrasto do Explorer sem um humano; isto só EXIBE o que
// `input.droppedCount/droppedPath/droppedX/droppedY` e
// `input.hoveredFiles/hoveredX/hoveredY` respondem, pra checar visual e no
// console arrastando um arquivo de verdade sobre a janela.
//   target/release/rts.exe run examples/claude-drop-files.ts

const app = createAppAt("Arraste um arquivo do Explorer para cá", 560, 320, 2150, 420);

while (app.running()) {
  if (!app.beginFrame()) break;
  app.fillRect(0, 0, 560, 320, 0x12161CFF);
  app.text(16, 12, "Arraste um arquivo do Explorer sobre esta janela.", 0xB0B8C0FF, 14);

  const hovered = input.hoveredFiles(app._win);
  const dropped = input.droppedCount(app._win);

  // realce do alvo de soltura enquanto algo paira.
  const targetColor = hovered > 0 ? 0x2244AAFF : 0x1A2230FF;
  app.box(16, 48, 528, 180, targetColor, hovered > 0 ? 3 : 1, 0x33445566, 8);
  app.text(
    30,
    60,
    "hoveredFiles=" + hovered + "  em " + input.hoveredX(app._win) + "," + input.hoveredY(app._win),
    0x99CCFFFF,
    14,
  );

  if (dropped > 0) {
    app.text(30, 84, "droppedCount=" + dropped + "  em " + input.droppedX(app._win) + "," + input.droppedY(app._win), 0xFFCC66FF, 14);
    let y = 108;
    for (let i = 0; i < dropped; i = i + 1) {
      const path = input.droppedPath(app._win, i);
      app.text(30, y, "[" + i + "] " + path, 0xFFFFFFFF, 13);
      console.log("dropped[" + i + "]=" + path);
      y = y + 18;
    }
  } else {
    app.text(30, 84, "(nenhum arquivo solto ainda)", 0x808890FF, 13);
  }

  app.endFrame();
}

app.close();
