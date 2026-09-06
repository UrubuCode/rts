import { describe, test, expect } from "rts:test";
import dom from "rts:dom";

// Um listener de DOM tem de sobreviver ao COLETOR — e não sobrevivia.
//
// O `Dom` guarda cada handler como um `i64` opaco (`ListenerRecord::callback`).
// O coletor deste motor descobre o que está vivo por duas vias: o que o
// `Context` segura, e uma varredura CONSERVATIVA da pilha da máquina. Um inteiro
// dentro de um `HashMap` do Rust não é nenhuma das duas — `roots::scan_stack`
// procura palavras que sejam referências CODIFICADAS, e um índice cru não o é.
// A célula do closure era varrida com o listener ainda registado, e a invocação
// seguinte encontrava o que tivesse ficado no lugar: `TypeError: object is not a
// function`, num `addEventListener` que nunca foi removido, sobre um elemento
// que continua na árvore.
//
// Este teste é a reprodução mínima, e o que ele pina é a ORDEM: registar,
// alocar o suficiente para o coletor correr, e só então despachar. Sem a
// alocação no meio ele passa mesmo com o defeito presente — foi assim que o
// defeito viveu tanto tempo, porque toda a gente que escreve um teste de
// eventos despacha logo a seguir a registar.
//
// O alcance era muito para além de um caso exótico: qualquer página com um
// relógio e um `addEventListener` perde os handlers ao fim de segundos. Foi
// encontrado num jogo em React que corre a 60 fps.

const doc = parseDocument("<div id='alvo'>alvo</div>");
const alvo = doc.getElementById("alvo");

let entregues = 0;
if (alvo !== null) {
  alvo.addEventListener("click", () => {
    entregues = entregues + 1;
  });
}

// Antes de despachar, ALOCAR. O número é generoso de propósito: tem de forçar
// pelo menos uma coleta em qualquer configuração de heap, e um teste que só
// falha em metade das máquinas não pina nada.
let lixo: any[] = [];
let i = 0;
while (i < 200000) {
  lixo.push({ a: i, s: "x" + i });
  if (i % 20000 === 0) lixo = [];
  i = i + 1;
}

let entreguesDepois = 0;
if (alvo !== null) {
  alvo.dispatchEvent("click");
  alvo.dispatchEvent("click");
  entreguesDepois = entregues;
}

// E o mesmo para um listener registado por um `<script>` da página, que é o
// caminho por onde uma página real regista os seus.
const pagina = parseDocument(
  "<button id='b'>x</button>" +
    "<script>" +
    "var n = 0;" +
    "document.getElementById('b').addEventListener('click', function () { n = n + 1; });" +
    "var lixo = [];" +
    "var i = 0;" +
    "while (i < 200000) { lixo.push({ a: i, s: 'x' + i }); if (i % 20000 === 0) { lixo = []; } i = i + 1; }" +
    "document.getElementById('b').dispatchEvent('click');" +
    "document.getElementById('st').textContent = '' + n;" +
    "</script>" +
    "<div id='st'>?</div>",
);
runScriptsAt(pagina, "https://localhost/");
const estado = pagina.getElementById("st");
const contadoNaPagina = estado === null ? "?" : estado.textContent;

describe("um listener sobrevive à coleta de lixo", () => {
  test("o callback continua a ser chamado depois de alocação pesada", () => {
    expect(entreguesDepois).toBe(2);
  });

  test("o mesmo por um <script> da página", () => {
    expect(contadoNaPagina).toBe("1");
  });
});
