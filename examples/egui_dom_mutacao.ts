import egui from "rts:egui";
import dom from "rts:dom";

// MUTAÇÃO do DOM via JS, no PIPELINE UNIFICADO: a árvore vive no rts-dom (headless,
// dom.parseHtml → handle), é mutada via dom.* (querySelector/setText/createElement/
// appendChild) SEM re-parsear, e o egui só LÊ e pinta via egui.render(win, d). O
// DOM é desacoplado do render — manipular a página não precisa de janela.

const NONE = -1; // sentinela "não encontrado" (invariante 3: -1, nunca u64::MAX)

// Layout/estilo DEFINIDOS NO DOM (rts-dom), não no egui.
dom.defineBlock("h1", 0, 26, 0, 4);
dom.defineBlock("p", 1, 0, 0, 0);
dom.defineBlock("ul", 0, 16, 0, 0);
dom.defineBlock("li", 1, 0, 1, 0);
dom.defineInline("b", 8);

// 1) Parseia o HTML UMA vez no rts-dom (a árvore retida vira a fonte da verdade).
const d = dom.parseHtml(
  "<h1 id='titulo'>Original</h1><p class='msg'>texto inicial</p><ul id='lista'></ul>"
);

// 2) MUTA a árvore via JS — sem re-parsear, direto no rts-dom (headless).
const titulo = dom.querySelector(d, "#titulo");
if (titulo !== NONE) dom.setText(d, titulo, "Titulo MUTADO via JS");

const msg = dom.querySelector(d, ".msg");
if (msg !== NONE) {
  dom.setText(d, msg, "texto trocado em runtime");
  dom.setAttr(d, msg, "class", "msg destaque");
}

// 3) Cria itens novos e anexa na lista (createElement + appendChild).
const lista = dom.querySelector(d, "#lista");
if (lista !== NONE) {
  let i = 1;
  while (i <= 3) {
    const li = dom.createElement(d, "li");
    dom.setText(d, li, "item criado " + i);
    dom.appendChild(d, lista, li);
    i = i + 1;
  }
}

// 4) Mostra a árvore DEPOIS das mutações (devtools-style, sem render).
dom.dump(d);

// 5) Loop de render — o egui só LÊ o DOM `d` do rts-dom e pinta.
const win = egui.openWindow("DOM mutacao (pipeline unificado)", 460, 360, 0);
while (egui.isOpen(win) !== 0) {
  if (egui.pump(win) !== 0) break;
  egui.beginFrame(win);
  egui.render(win, d);
  egui.endFrame(win);
}
egui.close(win);
dom.free(d);
