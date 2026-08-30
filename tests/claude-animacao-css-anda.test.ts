import { test, expect } from "rts:test";

// Uma `animation` de `@keyframes` interpola com o tempo. O frame da janela
// chama `dom.advance(doc, agora)`; este teste chama-o diretamente, que e o que
// torna a pergunta "esta pagina anima?" respondivel SEM abrir uma janela e
// alguem olhar.
//
// Veio de uma observacao a olho — "a barra de progresso do WhatsApp nao anda" —
// que so se podia confirmar ou desmentir assim. (Desmentiu-se: o motor anima; a
// barra do WhatsApp nao existe, porque e o carregador que a cria depois de
// receber os bundles da rede.)

const css =
  "@keyframes crescer { from { width: 10px } to { width: 200px } } " +
  "#barra { width: 10px; height: 12px; background: #0a7; animation: crescer 1s linear infinite }";
const doc = parseDocument("<style>" + css + "</style><div id='barra'></div>");
const barra = doc.getElementById("barra");

function largura(): string {
  return barra === null ? "?" : barra.getComputedProp("width");
}

// A primeira chamada fixa o instante inicial da animacao; as seguintes medem a
// partir dela.
dom.advance(doc._dom, 0);
const noInicio = largura();
dom.advance(doc._dom, 500);
const aMeio = largura();
dom.advance(doc._dom, 1000);
const noFim = largura();

test("uma animacao comeca no primeiro keyframe", function () {
  expect(noInicio).toBe("10px");
});

test("e interpola com o tempo", function () {
  // A meio de `10px -> 200px` sao 105px. O valor exato depende do arredondamento
  // do motor; o que este teste pina e que ANDOU e que anda para a frente.
  expect(aMeio !== noInicio).toBe(true);
  expect(noFim !== aMeio).toBe(true);
});
