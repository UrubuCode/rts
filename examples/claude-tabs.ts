// TABS + layout automático — abas que trocam de conteúdo, e componentes que se
// empilham sozinhos (column + auto*), sem posicionar x/y na mão. Mostra a
// ergonomia que "o pessoal" vai usar.
//   target/release/rts.exe run examples/claude-tabs.ts

const app = createApp("Tabs + layout automatico", 460, 380);
app.moveTo(2200, 360);

let activeTab = 0;       // qual aba está selecionada
let counter = 0;
let darkMode = 1;
let volume = 0.5;

while (app.running()) {
  if (!app.beginFrame()) break;

  app.fillRect(0, 0, 460, 380, 0x0E1218FF);

  // barra de abas (lado a lado). troca activeTab quando uma é clicada. (nomes
  // literais — array-indexado de string + uso interno baila no motor atual.)
  const tabW = 150;
  let a0 = 0; if (activeTab === 0) a0 = 1;
  let a1 = 0; if (activeTab === 1) a1 = 1;
  let a2 = 0; if (activeTab === 2) a2 = 1;
  if (app.tab(0, 0, tabW, 38, "Inicio", a0) !== 0) activeTab = 0;
  if (app.tab(tabW, 0, tabW, 38, "Ajustes", a1) !== 0) activeTab = 1;
  if (app.tab(tabW * 2, 0, tabW, 38, "Sobre", a2) !== 0) activeTab = 2;

  // conteúdo da aba ativa, empilhado AUTOMATICAMENTE (column + auto*)
  app.column(24, 60, 412);

  if (activeTab === 0) {
    app.autoLabel("Bem-vindo!", 0xFFFFFFFF);
    app.autoLabel("Contador: " + counter, 0x99CCFFFF);
    if (app.autoButton("Incrementar")) counter = counter + 1;
    if (app.autoButton("Resetar")) counter = 0;
  } else if (activeTab === 1) {
    app.autoLabel("Ajustes", 0xFFFFFFFF);
    darkMode = app.autoCheckbox(darkMode, "Modo escuro");
    app.autoLabel("Volume:", 0xC0C8D0FF);
    volume = app.autoSlider(volume, 0, 1);
    app.autoLabel("valor: " + volume, 0x99CCFFFF);
  } else {
    app.autoLabel("Sobre", 0xFFFFFFFF);
    app.autoLabel("UI imediata sobre render.* (backend egui).", 0xC0C8D0FF);
    app.autoLabel("Tabs + layout automatico + componentes.", 0xC0C8D0FF);
    app.autoLabel("O pessoal monta UI sem posicionar x/y.", 0x808890FF);
  }

  app.endFrame();
}

app.close();
