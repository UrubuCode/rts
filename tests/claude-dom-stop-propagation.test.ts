import { describe, test, expect } from "rts:test";

// stopPropagation / stopImmediatePropagation.
//
// Sem eles, um clique num botão dentro de um card dentro de um modal aciona os
// três handlers — e não há como um componente dizer "este evento é meu". É o
// idioma que faz menus e modais fecharem só quando devem.
//
// A distinção do DOM real é respeitada: `stopPropagation` deixa terminarem os
// outros listeners do MESMO nó e só corta a subida; `stopImmediatePropagation`
// corta na hora.

const html = "<html><head><style>div{display:block;}span{display:block;}</style></head><body>"
  + "<div id='avo'><div id='pai'><span id='filho'>alvo</span></div></div>"
  + "<p id='log'></p>"
  + "<div id='avo2'><div id='pai2'><span id='filho2'>alvo2</span></div></div>"
  + "<p id='log2'></p>"
  + "<div id='avo3'><span id='filho3'>alvo3</span></div>"
  + "<p id='log3'></p>"
  + "<script>"
  // ── caso 1: sem parar — o evento sobe até o avô
  + "  function marca(id, letra, alvoLog) {"
  + "    const el = document.getElementById(id);"
  + "    if (el === null) { return; }"
  + "    el.addEventListener('click', function (e) {"
  + "      const l = document.getElementById(alvoLog);"
  + "      if (l !== null) { l.setInnerHTML(l.textContent + letra); }"
  + "    });"
  + "  }"
  + "  marca('filho', 'F', 'log');"
  + "  marca('pai', 'P', 'log');"
  + "  marca('avo', 'A', 'log');"
  // ── caso 2: o PAI para a propagação — o avô não recebe
  + "  marca('filho2', 'F', 'log2');"
  + "  const p2 = document.getElementById('pai2');"
  + "  if (p2 !== null) { p2.addEventListener('click', function (e) {"
  + "    const l = document.getElementById('log2');"
  + "    if (l !== null) { l.setInnerHTML(l.textContent + 'P'); }"
  + "    e.stopPropagation();"
  + "  }); }"
  + "  marca('avo2', 'A', 'log2');"
  // ── caso 3: DOIS listeners no mesmo nó; o primeiro chama stopPropagation.
  //    O segundo do MESMO nó ainda roda; o avô não.
  + "  const f3 = document.getElementById('filho3');"
  + "  if (f3 !== null) {"
  + "    f3.addEventListener('click', function (e) {"
  + "      const l = document.getElementById('log3');"
  + "      if (l !== null) { l.setInnerHTML(l.textContent + '1'); }"
  + "      e.stopPropagation();"
  + "    });"
  + "    f3.addEventListener('click', function (e) {"
  + "      const l = document.getElementById('log3');"
  + "      if (l !== null) { l.setInnerHTML(l.textContent + '2'); }"
  + "    });"
  + "  }"
  + "  marca('avo3', 'A', 'log3');"
  + "</script>"
  + "</body></html>";

const doc = parseDocument(html);
runScripts(doc);

const f1 = doc.querySelector("#filho");
if (f1 !== null) f1.click();
const log1 = doc.textOf("#log");

const f2 = doc.querySelector("#filho2");
if (f2 !== null) f2.click();
const log2 = doc.textOf("#log2");

const f3 = doc.querySelector("#filho3");
if (f3 !== null) f3.click();
const log3 = doc.textOf("#log3");

describe("stopPropagation", () => {
  test("sem parar, o evento sobe: filho, pai, avo", () => {
    expect(log1).toBe("FPA");
  });

  test("stopPropagation no pai impede o avo de receber", () => {
    expect(log2).toBe("FP");
  });

  test("stopPropagation deixa terminar os listeners do MESMO no", () => {
    // '1' e '2' são do mesmo nó (ambos rodam); 'A' é o avô (cortado).
    expect(log3).toBe("12");
  });
});
