// Galeria de COMPONENTES — a biblioteca de UI imediata sobre o canvas/render.
// button, slider, checkbox, progressBar, panel, label: tudo via app.*, estilo
// imediato (desenha + interage + retorna). É o que "o pessoal" usa pra montar UI.
//   target/release/rts.exe run examples/claude-components.ts

const app = createApp("Componentes (UI imediata sobre render.*)", 520, 420);
app.moveTo(2000, 120);

// estado da UI (module-level — o dev guarda; UI imediata não guarda)
let counter = 0;
let volume = 0.5;
let brightness = 70;
let darkMode = 1;       // 0/1 (motor despacha melhor number que bool de método)
let notifications = 0;

while (app.running()) {
  if (!app.beginFrame()) break;

  let bg = 0x0E1218FF;
  if (darkMode === 0) bg = 0x1C2230FF;
  app.fillRect(0, 0, 520, 420, bg);
  app.label(20, 16, "Galeria de componentes", 0xFFFFFFFF);

  // painel "Acoes"
  app.panel(20, 48, 480, 90);
  app.label(36, 60, "Acoes", 0x99CCFFFF);
  if (app.button(36, 84, 140, 40, "Incrementar")) {
    counter = counter + 1;
  }
  if (app.button(190, 84, 140, 40, "Resetar")) {
    counter = 0;
  }
  app.label(346, 96, "Contador: " + counter, 0xFFFFFFFF);

  // painel "Ajustes" (sliders)
  app.panel(20, 152, 480, 120);
  app.label(36, 164, "Ajustes", 0x99CCFFFF);
  app.label(36, 190, "Volume", 0xC0C8D0FF);
  volume = app.slider(120, 188, 280, volume, 0, 1);
  app.label(412, 190, "" + volume, 0x99CCFFFF);
  app.label(36, 226, "Brilho", 0xC0C8D0FF);
  brightness = app.slider(120, 224, 280, brightness, 0, 100);
  app.label(412, 226, "" + brightness, 0x99CCFFFF);

  // painel "Opcoes" (checkboxes)
  app.panel(20, 286, 480, 80);
  app.label(36, 298, "Opcoes", 0x99CCFFFF);
  darkMode = app.checkbox(36, 322, darkMode, "Modo escuro");
  notifications = app.checkbox(220, 322, notifications, "Notificacoes");

  // barra de progresso reflete o brilho
  app.label(20, 380, "Brilho:", 0xC0C8D0FF);
  app.progressBar(90, 380, 410, 16, brightness / 100);

  app.endFrame();
}

app.close();
