// O jogo do dinossauro do Chrome, em React 18 real, na nossa janela.
//
// A pagina e `claude-dino.html`: os bundles UMD de producao do React e do
// ReactDOM, mais um componente que desenha o jogo com `<div>` posicionados em
// absoluto — NENHUM canvas. Um canvas seria uma superficie opaca e nao provaria
// nada sobre o motor; assim, cada cacto que anda e o `rts-dom` a refazer layout
// e o egui a pintar, sessenta vezes por segundo.
//
// O loop e o mesmo de `claude-react-janela.ts` e a ordem das tres bombas nao e
// arbitraria: teclado/edicao, cliques, temporizadores. E o `pumpTimerCallbacks`
// que faz o `setInterval` do jogo correr — sem ele a pagina monta, fica bonita
// e nao anda.
import egui from "rts:egui";
import dom from "rts:dom";
import { readFileSync } from "node:fs";

const ficheiro = "examples/dino/claude-dino.html";

const html = readFileSync(ficheiro, "utf8");
if (html.length === 0) {
  console.log("nao consegui ler " + ficheiro);
} else {
  console.log("[boot] " + ficheiro + ": " + html.length + " bytes");
  const doc = parseDocument(html);
  const d: i64 = doc._dom;
  console.log("[js] scripts corridos: " + runScriptsAt(doc, "https://localhost/"));

  // Os PIXEIS dos sprites. `loadResources` percorre as `<img>` do documento e
  // descodifica cada `src` de `data:image/…`, e tem de correr DEPOIS dos
  // scripts: as imagens do jogo sao criadas pelo React, nao existem no HTML
  // cru. Medido nesta sessao — antes desta chamada `imageNaturalWidth` de um
  // `<img>` criado em runtime responde 0, depois responde o tamanho real.
  //
  // Uma vez chega porque o jogo tem uma PISCINA FIXA de `<img>`: nada e criado
  // depois do primeiro render, so se liga e desliga `display`. Se algum dia
  // deixar de ser verdade, isto passa a ter de correr outra vez — e a forma de
  // dar por isso e um sprite que nao aparece, sem erro nenhum.
  // Bombear ANTES de carregar: o React 18 e concurrent, `render` so AGENDA o
  // trabalho, e as `<img>` do jogo so existem depois de o agendador correr.
  // Sem estas voltas, `loadResources` percorre um documento sem imagem nenhuma
  // e responde 0 — com o jogo a aparecer todo em branco e sem erro.
  let aquecer = 0;
  while (aquecer < 60) {
    pumpEventCallbacks(doc);
    pumpTimerCallbacks(doc);
    aquecer = aquecer + 1;
  }
  console.log("[img] sprites descodificados: " + loadResources(doc, "https://localhost/"));

  const win = egui.openWindow("RTS - Dino em React", 900, 520, 0);
  console.log("[win] handle=" + win + " aberta=" + egui.isOpen(win));

  let frames = 0;
  while (egui.isOpen(win)) {
    if (!egui.pump(win)) break;
    egui.beginFrame(win);
    egui.render(win, d);
    egui.endFrame(win);
    frames = frames + 1;
    pumpInputEvents(doc);
    pumpEventCallbacks(doc);
    pumpTimerCallbacks(doc);
  }
  egui.close(win);
  const fim = doc.getElementById("root");
  console.log("[fim] frames=" + frames + " filhos do #root="
    + (fim === null ? "?" : fim.children.length));
  console.log("[fim] texto:", fim === null ? "" : fim.textContent.substring(0, 160));
  dom.free(d);
}
