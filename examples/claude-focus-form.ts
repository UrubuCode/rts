import egui from "rts:egui";

// Camada ERGONÔMICA de input (fase 3): foco real + eventos por ID. DOIS campos de
// texto onde SÓ o clicado digita (foco), e botoes via clickable (estado de area).
// Tudo via app.* — sobre o input.* abstrato (que le o egui). A ergonomia organiza;
// o egui captura.
//   target/release/rts.exe run examples/claude-focus-form.ts

const app = createAppAt("Foco + eventos (form)", 480, 360, 2150, 420);

let nome = "";
let email = "";
let enviados = 0;

while (app.running()) {
  if (!app.beginFrame()) break;
  app.fillRect(0, 0, 480, 360, 0x12161CFF);
  app.text(20, 16, "Clique num campo e digite. So o focado recebe o teclado.", 0xB0B8C0FF, 14);

  app.text(20, 56, "Nome:", 0xC0C8D0FF, 15);
  nome = app.textField(1, 100, 50, 340, nome);

  app.text(20, 100, "Email:", 0xC0C8D0FF, 15);
  email = app.textField(2, 100, 94, 340, email);

  app.text(20, 140, "Focado (ID): " + app.focusedId(), 0x99CCFFFF, 13);

  // botao via clickable (estado: 0 idle 1 hover 2 pressed 3 CLICADO)
  const st = app.clickable(3, 100, 170, 160, 44);
  let bcolor = 0x2A3A50FF;
  if (st === 1) bcolor = 0x3A4F6EFF;
  if (st === 2) bcolor = 0x223040FF; // pressionando (mais escuro)
  app.box(100, 170, 160, 44, bcolor, 2, 0x6699CCFF, 8);
  app.text(130, 184, "Enviar", 0xFFFFFFFF, 16);
  if (st === 3) {
    enviados = enviados + 1;
  }

  app.box(20, 240, 440, 90, 0x1A2230FF, 1, 0x33445566 & 0xFFFFFFFF, 8);
  app.text(36, 252, "Nome: " + nome, 0xFFFFFFFF, 15);
  app.text(36, 276, "Email: " + email, 0xFFFFFFFF, 15);
  app.text(36, 300, "Enviados: " + enviados, 0xAAFFCCFF, 15);

  app.endFrame();
}

app.close();
