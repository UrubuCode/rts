// React 18 real, dos bundles UMD do CDN, a montar na nossa janela — e agora a
// RESPONDER ao rato.
//
// A diferenca para o headless e o LOOP: o React 18 e concurrent — `render`
// AGENDA e o trabalho corre depois, fatia a fatia. Sem um loop vivo ninguem o
// bombeia, e o `#root` fica vazio sem um erro. Aqui o frame do host bombeia.
//
// E a diferenca para a versao anterior deste ficheiro sao DUAS linhas:
// `pumpInputEvents` e `pumpEventCallbacks`. So `pumpTimerCallbacks` faz uma
// pagina ANIMAR e nao faz uma pagina RESPONDER — o clique do egui fica na fila
// crua e ninguem o despacha. Com elas, o clique entra por
// `__dispatchWithCallbacks`, que e o mesmo caminho de um `dispatchEvent` do
// prelude, e e la que o `event.target` e feito: e por isso que a cache de
// wrappers por `NodeId` e o que decide se o React consegue atribuir o clique a
// um componente.
import egui from "rts:egui";
import dom from "rts:dom";
import { readFileSync } from "node:fs";

const ficheiro = "react-app.html";

const html = readFileSync(ficheiro, "utf8");
if (html.length === 0) {
  console.log("nao consegui ler " + ficheiro);
} else {
  console.log("[boot] " + ficheiro + ": " + html.length + " bytes");
  const doc = parseDocument(html);
  const d: i64 = doc._dom;
  console.log("[js] scripts corridos: " + runScriptsAt(doc, "https://localhost/"));

  const win = egui.openWindow("RTS - React", 900, 700, 0);
  console.log("[win] handle=" + win + " aberta=" + egui.isOpen(win));
  // `isOpen`/`pump` respondem BOOLEANOS, e `pump` responde `true` para
  // CONTINUAR (do lado Rust e `from_bool(pump(...) == 0)`). O idioma
  // `if (egui.pump(win) !== 0) break;` estava em 45 ficheiros deste repo: le
  // `true !== 0` como verdadeiro e sai no primeiro frame, com a janela aberta e
  // sem nunca desenhar nada. Foram todos corrigidos na mesma passagem.
  let frames = 0;
  while (egui.isOpen(win)) {
    if (!egui.pump(win)) break;
    egui.beginFrame(win);
    egui.render(win, d);
    // A barra de diagnostico: o que o React montou, contado do DOM em vez de
    // acreditado. Um `#root` com zero filhos e a forma que uma montagem falhada
    // tem aqui — nao ha erro nenhum a acompanha-la.
    const raiz = doc.getElementById("root");
    egui.drawText(win, "rts-dom - React 18 - filhos do #root: "
      + (raiz === null ? 0 : raiz.children.length) + " - frame " + frames, 0);
    egui.endFrame(win);
    frames = frames + 1;
    // A mesma ordem de um frame do mini-browser: teclado/edicao, cliques, timers.
    pumpInputEvents(doc);
    pumpEventCallbacks(doc);
    pumpTimerCallbacks(doc);
  }
  egui.close(win);
  const fim = doc.getElementById("root");
  console.log("[fim] frames=" + frames + " filhos do #root="
    + (fim === null ? "?" : fim.children.length));
  console.log("[fim] texto:", fim === null ? "" : fim.textContent.substring(0, 120));
  dom.free(d);
}
