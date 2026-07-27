import { describe, test, expect } from "rts:test";

// Clique com AÇÃO DEFAULT — o que faz botões e links de uma página funcionarem.
//
// O modelo é o do browser: hit-test acha o alvo, o evento `click` é despachado COM
// bubbling (os listeners que os <script> da página registraram disparam), e só
// então roda a ação default (navegar quando o alvo está dentro de um <a href>) —
// a menos que algum listener tenha chamado `preventDefault()`.
//
// Valores pré-computados no top-level (regra da suíte: chamar método dentro do
// closure de test() pode pegar handle já coletado pelo GC).

const VW = 800;

// Página: um link com <span> dentro (o clique cai no span, deve achar o <a>),
// um botão com listener, e um link que cancela a navegação.
//
// `display:block` aqui é para testar o aninhamento <a> > <span> com rects
// IDÊNTICOS (o caso que exigiu o desempate por profundidade no hit-test). O caso
// INLINE — `<a>` no meio de um parágrafo, que é como uma página real escreve — é
// coberto pelo describe seguinte.
const html = "<html><head><style>"
  + "a { display: block; } button { display: block; } span { display: block; }"
  + "</style></head><body>"
  + "<div id='wrap'>"
  + "<a id='lnk' href='/destino'><span id='inner'>ir para destino</span></a>"
  + "<button id='btn'>clicar</button>"
  + "<a id='spa' href='#'>link de SPA</a>"
  + "</div>"
  + "<script>"
  + "  const b = document.getElementById('btn');"
  + "  if (b !== null) { b.addEventListener('click', function (e) {"
  + "    const t = document.getElementById('btn');"
  + "    if (t !== null) { t.setInnerHTML('CLICADO'); }"
  + "  }); }"
  + "  const s = document.getElementById('spa');"
  + "  if (s !== null) { s.addEventListener('click', function (e) {"
  + "    e.preventDefault();"
  + "    const w = document.getElementById('wrap');"
  + "    if (w !== null) { w.setAttribute('data-spa', 'sim'); }"
  + "  }); }"
  + "</script>"
  + "</body></html>";

const doc = parseDocument(html);
runScripts(doc);

const inner = doc.querySelector("#inner");
const btn = doc.querySelector("#btn");
const spa = doc.querySelector("#spa");

// Acha o Y em que um dado id está, varrendo a coluna x=3. Evita depender de
// `offsetTop` (que a fachada ainda não expõe) e de constantes de layout que
// mudariam com qualquer ajuste de margem/padding.
function yDoNo(id: string): number {
  let y = 0;
  while (y < 400) {
    const e = doc.elementFromPoint(3, y, VW);
    if (e !== null && e.getAttribute("id") === id) return y;
    y = y + 2;
  }
  return -1;
}

const innerY = yDoNo("inner");
const btnY = yDoNo("btn");
const spaY = yDoNo("spa");
const innerX = 3;
const btnX = 3;
const spaX = 3;

// 1) hit-test genérico acha o nó MAIS PROFUNDO (o <span>, não o <a> nem o <div>)
const hitEl = doc.elementFromPoint(innerX, innerY, VW);
const hitId = hitEl === null ? "" : hitEl.getAttribute("id");

// 2) clicar no <span> DENTRO do link devolve o href do <a> (closest subiu)
const hrefFromSpan = doc.clickAt(innerX, innerY, VW);

// 3) clicar no botão dispara o listener da página (que muda o texto)
const btnHref = doc.clickAt(btnX, btnY, VW);
const btnAfter = btn === null ? "" : btn.textContent;

// 4) preventDefault cancela a navegação, mas o listener AINDA roda
const spaHref = doc.clickAt(spaX, spaY, VW);
const wrap = doc.querySelector("#wrap");
const spaMarked = wrap === null ? "" : wrap.getAttribute("data-spa");

// 5) clique no vazio não acha nada
const emptyHref = doc.clickAt(5, 3000, VW);

// ── Link INLINE (o caso de uma página real: <a> sem display:block) ─────────────
// Um `<a>` no meio de um parágrafo agora tem rect próprio em `node_rects`, então
// o clique o encontra e navega — e o clique AO LADO dele, no mesmo parágrafo, não.
const htmlInline = "<html><body>"
  + "<p id='par'>Antes do <a id='il' href='/destino-inline'>link aqui</a> e depois.</p>"
  + "</body></html>";
const docIn = parseDocument(htmlInline);

// Acha uma coordenada DENTRO do <a> varrendo a linha do parágrafo.
function xDoLink(): number {
  let x = 0;
  while (x < 600) {
    const e = docIn.elementFromPoint(x, 20, VW);
    if (e !== null && e.getAttribute("id") === "il") return x;
    x = x + 4;
  }
  return -1;
}
const xLink = xDoLink();
const hrefInline = xLink < 0 ? "" : docIn.clickAt(xLink, 20, VW);
const hrefAoLado = docIn.clickAt(2, 20, VW);

describe("clique em link INLINE (página real)", () => {
  test("o <a> inline tem caixa própria e é encontrado", () => {
    expect(xLink >= 0).toBe(true);
  });

  test("clicar no link inline navega", () => {
    expect(hrefInline).toBe("/destino-inline");
  });

  test("clicar no texto AO LADO do link não navega", () => {
    expect(hrefAoLado).toBe("");
  });
});

describe("clique: hit-test + evento + ação default", () => {
  test("elementFromPoint devolve o nó mais profundo", () => {
    expect(hitId).toBe("inner");
  });

  test("clicar dentro do link navega (closest acha o <a>)", () => {
    expect(hrefFromSpan).toBe("/destino");
  });

  test("clicar no botão dispara o listener do <script> da página", () => {
    expect(btnAfter).toBe("CLICADO");
  });

  test("botão sem <a> ancestral não navega", () => {
    expect(btnHref).toBe("");
  });

  test("preventDefault cancela a navegação", () => {
    expect(spaHref).toBe("");
  });

  test("listener roda mesmo cancelando a ação default", () => {
    expect(spaMarked).toBe("sim");
  });

  test("clique no vazio não navega", () => {
    expect(emptyHref).toBe("");
  });
});
