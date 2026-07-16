import { describe, test, expect } from "rts:test";
import dom from "rts:dom";

// Eventos com CALLBACK real (modelo do browser): addEventListener(type, fn) +
// dispatchEvent invocando fn({type, target, currentTarget}) com bubbling.
// Pré-computado no top-level (regra do projeto).

const doc = parseDocument("<div id='pai'><button id='b'>x</button></div>");
const btn = doc.getElementById("b");
const pai = doc.getElementById("pai");

let cliques = 0;
let tipoVisto = "";
let alvoTag = "";
let bubbleCurrent = "";

if (btn !== null) {
  btn.addEventListener("click", (ev: any) => {
    cliques = cliques + 1;
    tipoVisto = ev.type;
    alvoTag = ev.target.tagName;
  });
}
if (pai !== null) {
  pai.addEventListener("click", (ev: any) => {
    bubbleCurrent = ev.currentTarget.tagName;
  });
}
let notificados = 0;
if (btn !== null) {
  notificados = btn.dispatchEvent("click");
  btn.dispatchEvent("click");
}

// listener registrado por um <script> de página
const html2 = "<button id='salvar'>s</button><div id='st'>antes</div>" +
  "<script>" +
  "const b = document.getElementById('salvar');" +
  "if (b !== null) {" +
  "  b.addEventListener('click', function(ev) {" +
  "    const s = document.getElementById('st');" +
  "    if (s !== null) { s.textContent = 'salvo:' + ev.type; }" +
  "  });" +
  "}" +
  "</script>";
const doc2 = parseDocument(html2);
runScripts(doc2);
const b2 = doc2.getElementById("salvar");
if (b2 !== null) { b2.dispatchEvent("click"); }
const st = doc2.getElementById("st");
const stDepois = st === null ? "" : st.textContent;

// ciclo do BACKEND (hit-test simulado): pushRawEvent → pumpEventCallbacks →
// callback do script roda (o mesmo caminho que o clique real do mouse percorre).
const doc3 = parseDocument(
  "<button id='x'>x</button>" +
  "<script>" +
  "const b = document.getElementById('x');" +
  "if (b !== null) { b.addEventListener('click', function(ev) { b.setAttribute('data-ok', '1'); }); }" +
  "</script>");
runScripts(doc3);
const bx = doc3.getElementById("x");
let pumped = 0;
if (bx !== null) {
  dom.pushRawEvent(doc3._dom, bx.nodeId, "click");
  pumped = pumpEventCallbacks(doc3);
}
const bx2 = doc3.getElementById("x");
const okAttr = bx2 === null ? "" : bx2.getAttribute("data-ok");

describe("eventos com callback (modelo do browser)", () => {
  test("callback dispara com objeto de evento", () => {
    expect(cliques).toBe(2);
    expect(tipoVisto).toBe("click");
    expect(alvoTag).toBe("BUTTON");
  });
  test("bubbling notifica o pai com currentTarget correto", () => {
    expect(bubbleCurrent).toBe("DIV");
    expect(notificados).toBe(2); // alvo + pai
  });
  test("listener registrado por <script> de página dispara", () => {
    expect(stDepois).toBe("salvo:click");
  });
  test("evento do backend (hit-test) chega no callback via pump", () => {
    expect(pumped).toBe(1);
    expect(okAttr).toBe("1");
  });
});
