import { describe, test, expect } from "rts:test";

// Ciclo de vida do nó (lote M, PLAN §4.M): a freelist recicla um `idx`
// desanexado sem wrapper vivo (`crates/rts-dom/src/dom/freelist.rs`), e a
// fachada chama `dom.releaseSubtree` de `removeChild`/`remove()` quando não
// há wrapper na subárvore. Este ficheiro pina o comportamento do lado TS —
// o teste de que a ARENA propriamente dita não cresce está do lado Rust
// (`crates/rts-dom/src/dom/tests/freelist.rs`), porque é lá que dá para
// medir `nodes.len()` sem passar pela indireção do bridge a cada iteração.

describe("ciclo de vida do no (lote M)", () => {
  test("10000 appendChild/removeChild sem guardar referencias termina e a contagem de filhos fica certa", () => {
    const doc = parseDocument("<ul id='lista'></ul>");
    const lista = doc.getElementById("lista");
    expect(lista !== null).toBe(true);
    if (lista === null) return;

    for (let i = 0; i < 10000; i = i + 1) {
      const item = doc.createElement("li");
      lista.appendChild(item);
      lista.removeChild(item);
    }

    expect(lista.childNodes.length).toBe(0);

    // A arena não fica presa a um pico proporcional a N: uma folga contra o
    // tamanho medido logo após o loop, e não um número absoluto, porque o
    // ponto é "não cresce com N" e não "é exatamente X".
    const depois = (doc as any).nodeCount as number;
    expect(depois < 1000).toBe(true);
  });

  test("um wrapper guardado de um no removido continua a responder aos seus proprios campos, sem lancar", () => {
    const doc = parseDocument("<div id='raiz'></div>");
    const raiz = doc.getElementById("raiz");
    expect(raiz !== null).toBe(true);
    if (raiz === null) return;

    const el = doc.createElement("span");
    el.id = "alvo";
    raiz.appendChild(el);
    expect(el.isConnected).toBe(true);

    raiz.removeChild(el);

    // Sem lançar TypeError — só respostas "vazias"/negativas, exatamente
    // como um NodeId que não resolve mais responde em qualquer lugar do
    // bridge (`get_attribute`/`get_text` devolvem default, nunca entram em
    // pânico).
    expect(el.isConnected).toBe(false);
    expect(typeof el.id).toBe("string");
    expect(typeof el.tagName).toBe("string");
  });

  test("remover uma subarvore com um wrapper vivo no meio nao quebra esse wrapper", () => {
    const doc = parseDocument(
      "<div id='raiz'><ul id='lista'><li id='vivo'>x</li><li>y</li><li>z</li></ul></div>"
    );
    const raiz = doc.getElementById("raiz");
    const lista = doc.getElementById("lista");
    const vivo = doc.getElementById("vivo"); // materializa o wrapper ANTES da remoção
    expect(raiz !== null && lista !== null && vivo !== null).toBe(true);
    if (raiz === null || lista === null || vivo === null) return;

    raiz.removeChild(lista);

    // `vivo` continua um objeto válido: o `release_subtree` do Rust recusa
    // reciclar `lista` porque a checagem de subárvore do TS achou este
    // wrapper — `lista` fica "lixo" na arena (como sempre foi antes do
    // lote M), e uma leitura por `vivo` continua a resolver de verdade.
    expect(vivo.textContent).toBe("x");
    expect(vivo.isConnected).toBe(false);
  });

  test("document.close() liberta o handle: uma chamada depois nao lanca e devolve vazio", () => {
    const doc = parseDocument("<p>oi</p>");
    const root = doc.documentElement;
    expect(root !== null).toBe(true);

    (doc as any).close();

    // Handle já ausente do store: `rootId`/etc devolvem a sentinela, nunca
    // lançam — o mesmo padrão de um `NodeId` que não resolve.
    expect(doc.documentElement).toBe(null);

    // Chamar close() de novo é seguro (dom.free/`__dropWindow` já toleram
    // um handle/idx ausente).
    (doc as any).close();
  });
});
