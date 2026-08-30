import { test, expect } from "rts:test";

// Um `var` de uma funcao e dessa funcao, e uma reatribuicao DENTRO de um bloco
// escreve nesse binding — nao num nome de fora que por acaso se chama igual.
//
// FIXTURE VERMELHA: pina um defeito ABERTO, e so aparece num `<script>` de
// pagina. Num modulo passa, porque o escopo de um modulo nao tem `parent` nem
// `top` nem `name`; o de uma pagina TEM, porque o escopo de uma pagina E o
// `window` (ver `docs/ui/page-script-bridge.md`).
//
// Veio de um caso real. O React 18 anda a arvore de fibers assim:
//
//     var parent = sourceFiber.return;
//     while (parent !== null) { ...; parent = parent.return; }
//
// A DECLARACAO e local e esta certa. A REATRIBUICAO dentro do `while` ia para
// `window.parent`, que e `[Replaceable]` e ignora a escrita; a leitura seguinte
// devolvia o `window`, o laco nunca terminava, e o erro que chegava ao
// utilizador era `undefined.childLanes` — trinta ficheiros a jusante da causa.

const html =
  "<div id='saida'>-</div><script>" +
  "function anda(inicio) {" +
  "  var parent = inicio.pai;" +
  "  var passos = 0;" +
  "  while (parent !== null) {" +
  "    passos = passos + 1;" +
  "    if (passos > 5) { return -1; }" +
  "    parent = parent.pai;" +
  "  }" +
  "  return passos;" +
  "}" +
  "var raiz = { pai: null };" +
  "var meio = { pai: raiz };" +
  "var folha = { pai: meio };" +
  "var el = document.getElementById('saida');" +
  "if (el !== null) { el.setInnerHTML('' + anda(folha)); }" +
  "</script>";

const doc = parseDocument(html);
runScripts(doc);
const saida = doc.getElementById("saida");
const texto = saida === null ? "(sem no)" : saida.textContent;

test("um var reatribuido dentro de um while continua a ser o da funcao", function () {
  // `-1` significa que o laco nunca terminou: `parent` deixou de ser o local.
  expect(texto).toBe("2");
});
