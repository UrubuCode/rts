import { describe, test, expect } from "rts:test";

// Scroll no documento (lote G, #2621): scrollTop/scrollLeft/scrollWidth/
// scrollHeight/clientWidth/clientHeight/scrollTo/scrollBy/scrollIntoView por
// elemento, e scrollTo/scrollBy/scrollX/scrollY/pageXOffset/pageYOffset na
// janela — tudo por cima do primitivo novo em `rts:dom` (o offset vive no
// `Dom`, não mais só no backend egui).

const doc = parseDocument(
  "<div id='caixa' style='height:100px;overflow:auto'>" +
  "<div id='filho' style='height:300px'>conteudo</div>" +
  "</div>"
);
const caixa = doc.getElementById("caixa");
const filho = doc.getElementById("filho");

let scrollTopLido = -1;
let scrollHeightAntes = -1;
let clientHeightAntes = -1;
let scrolledEventos = 0;

if (caixa !== null) {
  caixa.addEventListener("scroll", (_event: any) => { scrolledEventos = scrolledEventos + 1; });
  scrollHeightAntes = caixa.scrollHeight;
  clientHeightAntes = caixa.clientHeight;
  caixa.scrollTop = 50;
  scrollTopLido = caixa.scrollTop;
}
const eventosPumped = caixa !== null ? pumpEventCallbacks(doc) : 0;

// clamp: pedir mais do que o conteúdo permite volta o teto (300 - 100 = 200).
let scrollTopClampado = -1;
if (caixa !== null) {
  caixa.scrollTop = 999999;
  scrollTopClampado = caixa.scrollTop;
}

// scrollBy soma ao offset actual (e clampa igual a scrollTop=).
let scrollTopAposScrollBy = -1;
if (caixa !== null) {
  caixa.scrollTop = 50;
  caixa.scrollBy(0, 20);
  scrollTopAposScrollBy = caixa.scrollTop;
}

// scrollIntoView: alinha o topo do filho (único filho, no topo do conteúdo)
// com o topo da região — partindo de um scroll bem longe de zero.
let scrollTopAposIntoView = -1;
if (caixa !== null && filho !== null) {
  caixa.scrollTop = 100;
  filho.scrollIntoView();
  scrollTopAposIntoView = caixa.scrollTop;
}

// window: scrollTo/scrollBy/scrollX/scrollY/pageXOffset/pageYOffset — o
// mesmo `Dom` por baixo, agora para a PÁGINA em vez de uma região.
const winDoc = parseDocument("<div style='height:2000px'>alto</div>");
const win = __makeWindow(winDoc._dom, "https://exemplo.com/", 1000, 800);
let winScrollEventos = 0;
win.addEventListener("scroll", (_event: any) => { winScrollEventos = winScrollEventos + 1; });
win.scrollTo(0, 100);
pumpEventCallbacks(winDoc);
const winScrollY = win.scrollY;
const winPageYOffset = win.pageYOffset;
win.scrollBy(0, 10);
const winScrollYDepoisDeBy = win.scrollY;

describe("scroll no documento (lote G)", () => {
  test("scrollTop escreve e lê", () => {
    expect(scrollTopLido).toBe(50);
  });

  test("scrollHeight excede clientHeight numa região com overflow", () => {
    expect(scrollHeightAntes > clientHeightAntes).toBe(true);
    expect(clientHeightAntes).toBe(100);
  });

  test("evento scroll dispara uma vez por mudança", () => {
    expect(eventosPumped).toBe(1);
    expect(scrolledEventos).toBe(1);
  });

  test("scrollTop clampa ao conteúdo", () => {
    expect(scrollTopClampado).toBe(200);
  });

  test("scrollBy soma ao offset actual", () => {
    expect(scrollTopAposScrollBy).toBe(70);
  });

  test("scrollIntoView alinha o topo do filho com o topo da região", () => {
    expect(scrollTopAposIntoView).toBe(0);
  });

  test("window.scrollTo/scrollY/pageYOffset reais", () => {
    expect(winScrollY).toBe(100);
    expect(winPageYOffset).toBe(100);
    expect(winScrollEventos).toBe(1);
  });

  test("window.scrollBy soma ao offset actual", () => {
    expect(winScrollYDepoisDeBy).toBe(110);
  });
});
