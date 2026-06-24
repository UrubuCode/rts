import egui from "rts:egui";

class Window {
  __h: number;
  constructor(t: string, w: number, h: number) { this.__h = egui.openWindow(t, w, h, 0); }
  isOpen(): boolean { return egui.isOpen(this.__h) !== 0; }
  pump(): boolean { return egui.pump(this.__h) === 0; }
  beginFrame(): void { egui.beginFrame(this.__h); }
  endFrame(): void { egui.endFrame(this.__h); }
  html(s: string): void { egui.html(this.__h, s); }
  dumpDom(): void { egui.domDump(this.__h); }
  close(): void { egui.close(this.__h); }
}

// Blocos (top-level, literais — workaround do #1726).
egui.defineBlock("h1", 0, 26, 0, 4);
egui.defineBlock("p", 1, 0, 0, 0);
egui.defineBlock("ul", 0, 16, 0, 0);
egui.defineBlock("li", 1, 0, 1, 0);
egui.defineInline("b", 8);

const NONE = -1; // sentinela "não encontrado" (invariante 3: -1, nunca u64::MAX)

const win = new Window("DOM mutacao", 460, 360);

// 1) Parseia o HTML UMA vez (a árvore retida vira a fonte da verdade).
win.beginFrame();
win.html("<h1 id='titulo'>Original</h1><p class='msg'>texto inicial</p><ul id='lista'></ul>");
win.endFrame();

// 2) MUTA a árvore via JS — sem re-parsear HTML.
const titulo = egui.querySelector(win.__h, "#titulo");
if (titulo !== NONE) egui.setText(win.__h, titulo, "Titulo MUTADO via JS");

const msg = egui.querySelector(win.__h, ".msg");
if (msg !== NONE) {
  egui.setText(win.__h, msg, "texto trocado em runtime");
  egui.setAttr(win.__h, msg, "class", "msg destaque");
}

// 3) Cria itens novos e anexa na lista (createElement + appendChild).
const lista = egui.querySelector(win.__h, "#lista");
if (lista !== NONE) {
  let i = 1;
  while (i <= 3) {
    const li = egui.createElement(win.__h, "li");
    egui.setText(win.__h, li, "item criado " + i);
    egui.appendChild(win.__h, lista, li);
    i = i + 1;
  }
}

// 4) Mostra a árvore DEPOIS das mutações.
win.dumpDom();

// 5) Loop de render — NÃO chama html() de novo, então as mutações persistem.
while (win.isOpen()) {
  if (!win.pump()) break;
  win.beginFrame();
  win.endFrame();
}
win.close();
