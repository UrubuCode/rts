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
// `display:block` nos três é NECESSÁRIO hoje: só elementos de BLOCO entram em
// `node_rects` (o layout documenta: "nós inline/texto não entram — um inline teria
// múltiplos rects, fase futura"), e sem rect não há hit-test. Como `<a>`/`<button>`/
// `<span>` são inline por padrão, um clique neles num HTML real ainda não acha o
// alvo. Está declarado como limitação conhecida, não contornado em silêncio.
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
