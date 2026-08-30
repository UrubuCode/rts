// O Preact 10.24.3 real do CDN, na nossa janela.
//
// Existe separado do `claude-react-janela.ts` porque os dois reconciliadores
// usam o DOM de maneiras DIFERENTES, e cada um encontra o que falta ao outro.
// O Preact precisou de tres coisas que o React nao usa — `on<evento>` como
// propriedade (o `in` decide o nome do evento que ele regista), `this` ligado
// ao no (o despachante dele mora la) e `no.data` para o texto (o React escreve
// `nodeValue`). As tres estao pinadas em
// `tests/claude-preact-precisa-destas-tres.test.ts`.
//
// A pagina monta-se com `scratchpad/buscar-preact.ts`, que vai busca-la ao
// CDN com o nosso proprio `fetch`.
import egui from "rts:egui";
import dom from "rts:dom";
import { readFileSync } from "node:fs";

const ficheiro = "preact-app.html";

const html = readFileSync(ficheiro, "utf8");
if (html.length === 0) {
  console.log("nao consegui ler " + ficheiro);
} else {
  console.log("[boot] " + ficheiro + ": " + html.length + " bytes");
  const doc = parseDocument(html);
  const d: i64 = doc._dom;
  console.log("[js] scripts corridos: " + runScriptsAt(doc, "https://localhost/"));

  const win = egui.openWindow("RTS - Preact", 900, 700, 0);
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
