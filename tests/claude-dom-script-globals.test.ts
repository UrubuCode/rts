import { describe, test, expect } from "rts:test";

// ESCOPO GLOBAL COMPARTILHADO ENTRE OS <script> DA MESMA PÁGINA.
//
// Num browser todos os <script> de um documento falam com UM objeto global: o
// script A define `foo = ...` e o script B, compilado depois, enxerga. Antes
// desta fatia cada <script> era uma ilha (um `window` novo por script), e um
// loader definido no script 2 morria com ele — o que reprovava o boot de páginas
// reais (WhatsApp/Meta: 1 de 33 scripts rodava; os outros 28 morriam em
// "unknown function requireLazy").
//
// Três mecanismos entram aqui, todos na borda do DOM (o motor não foi afrouxado):
//   1. `window` PERSISTENTE por documento + saco de globais `__G` compartilhado;
//   2. GLOBAL IMPLÍCITO (`x = 1` sem var/let/const) reescrito para `__G.x` — o
//      compilador do RTS recusa criar binding implícito, e com razão; a página é
//      que tem semântica sloppy, então a tradução mora aqui;
//   3. duas normalizações sintáticas de JS minificado: `arguments` → rest param
//      e sequência `a=1,b=function(){}` → statements separados.
//
// Pré-computado no top-level (regra do projeto: método dentro de test() pode
// perder handle pro GC).

// ── 1. um script define, o SEGUINTE consome ──────────────────────────────────
const htmlCompart =
  "<div id='out'>vazio</div>" +
  "<script>publicado = 'definido no script 1';</script>" +
  "<script>" +
  "  const el = document.getElementById('out');" +
  "  if (el !== null) { el.setInnerHTML(publicado); }" +
  "</script>";
const docCompart = parseDocument(htmlCompart);
const ranCompart = runScripts(docCompart);
const outCompart = docCompart.getElementById("out");
const textoCompart = outCompart === null ? "" : outCompart.textContent;

// ── 2. FUNÇÃO global publicada e CHAMADA por outro script ────────────────────
// É a forma exata do loader da Meta: `requireLazy=function(){stub.push(arguments)}`
// (sequência com vírgula + `arguments`), publicado num script e chamado noutro.
const htmlLoader =
  "<div id='r'>0</div>" +
  "<script>__stub=[],carrega=function(){__stub.push(arguments)};</script>" +
  "<script>carrega(['Modulo'], function(m){ return 0; }, null, 256);</script>" +
  "<script>" +
  "  const el = document.getElementById('r');" +
  "  if (el !== null) { el.setInnerHTML('' + __stub.length); }" +
  "</script>";
const docLoader = parseDocument(htmlLoader);
const ranLoader = runScripts(docLoader);
const outLoader = docLoader.getElementById("r");
const textoLoader = outLoader === null ? "" : outLoader.textContent;

// ── 3. `window.x` de um script é visto pelo próximo (window persistente) ─────
const htmlWin =
  "<div id='w'>vazio</div>" +
  "<script>window.marcado = 'via window';</script>" +
  "<script>" +
  "  const el = document.getElementById('w');" +
  "  if (el !== null) { el.setInnerHTML(window.marcado); }" +
  "</script>";
const docWin = parseDocument(htmlWin);
const ranWin = runScripts(docWin);
const outWin = docWin.getElementById("w");
const textoWin = outWin === null ? "" : outWin.textContent;

// ── 4. NÃO-REGRESSÃO: local reatribuído não é global implícito ───────────────
// `let i = 0; … i = i + 1` — o `i` do segundo statement parece atribuição a nome
// livre; se o scanner o tratasse como global, o loop quebraria. Cobre o bug que
// esta fatia introduziu e corrigiu (filtro de nomes declarados).
const htmlLocal =
  "<div id='l'>x</div><ul id='lista'></ul>" +
  "<script>" +
  "  let i = 0;" +
  "  while (i < 3) {" +
  "    const li = document.createElement('li');" +
  "    li.setInnerHTML('item ' + i);" +
  "    const lst = document.getElementById('lista');" +
  "    if (lst !== null) { lst.appendChild(li); }" +
  "    i = i + 1;" +
  "  }" +
  "</script>";
const docLocal = parseDocument(htmlLocal);
const ranLocal = runScripts(docLocal);
const lisLocal = docLocal.querySelectorAll("#lista li");
const nLisLocal = lisLocal.length;
const ultimoLocal = nLisLocal === 3 ? lisLocal[2].textContent : "";

// ── 5. SHADOWING: declaração local vence o global de mesmo nome ──────────────
const htmlShadow =
  "<div id='s'>vazio</div>" +
  "<script>nome = 'global';</script>" +
  "<script>" +
  "  const nome = 'local';" +
  "  const el = document.getElementById('s');" +
  "  if (el !== null) { el.setInnerHTML(nome); }" +
  "</script>";
const docShadow = parseDocument(htmlShadow);
const ranShadow = runScripts(docShadow);
const outShadow = docShadow.getElementById("s");
const textoShadow = outShadow === null ? "" : outShadow.textContent;

// ── 6. ISOLAMENTO ENTRE DOCUMENTOS: o global de um doc não vaza pro outro ────
const docA = parseDocument("<div id='a'>x</div><script>soDoDocA = 'A';</script>");
runScripts(docA);
const docB = parseDocument(
  "<div id='b'>intacto</div>" +
  "<script>" +
  "  const el = document.getElementById('b');" +
  "  if (el !== null) { el.setInnerHTML('B nao viu A'); }" +
  "</script>");
const ranB = runScripts(docB);
const outB = docB.getElementById("b");
const textoB = outB === null ? "" : outB.textContent;

describe("escopo global compartilhado entre <script> da página", () => {
  test("script publica global; o seguinte lê", () => {
    expect(ranCompart).toBe(2);
    expect(textoCompart).toBe("definido no script 1");
  });

  test("função global (sequência + arguments) é chamada por outro script", () => {
    expect(ranLoader).toBe(3);
    expect(textoLoader).toBe("1");
  });

  test("window é o MESMO objeto entre scripts do documento", () => {
    expect(ranWin).toBe(2);
    expect(textoWin).toBe("via window");
  });

  test("local reatribuído não vira global implícito", () => {
    expect(ranLocal).toBe(1);
    expect(nLisLocal).toBe(3);
    expect(ultimoLocal).toBe("item 2");
  });

  test("declaração local faz shadow do global", () => {
    expect(ranShadow).toBe(2);
    expect(textoShadow).toBe("local");
  });

  test("documentos diferentes têm escopos globais isolados", () => {
    expect(ranB).toBe(1);
    expect(textoB).toBe("B nao viu A");
  });
});
