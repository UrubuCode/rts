import { describe, test, expect } from "rts:test";

// AUTOMAÇÃO HEADLESS — dirigir uma página por código, sem browser externo.
//
// É o papel que hoje se pede a um Chrome + Puppeteer: abrir a página, achar
// elementos por seletor, preencher campos, clicar, e conferir o resultado. Aqui
// isso roda IN-PROCESS, sem janela, sem backend de input, sem processo externo.
//
// Dois níveis de fidelidade, ambos exercitados abaixo:
//   • por SELETOR (`doc.click("#btn")`) — o que um script de automação quer
//     escrever: direto ao ponto, sem depender de geometria.
//   • por GESTO (`doc.mouse.click(x,y)`, `doc.keyboard.press("a")`) — simula o
//     USUÁRIO: passa pelo hit-test e pela sequência real de eventos
//     (move→down→up→click, keydown→ação→keyup), então exercita os mesmos
//     caminhos que uma pessoa exercitaria.

const VW = 800;

const html = "<html><body>"
  + "<form id='f'>"
  + "<input id='nome' type='text' value='' />"
  + "<p id='saida'>vazio</p>"
  + "<p id='log'>-</p>"
  + "<p id='par'>texto com <a id='lnk' href='/ok'>um link</a> no meio.</p>"
  + "</form>"
  + "<script>"
  + "  const inp = document.getElementById('nome');"
  + "  if (inp !== null) { inp.addEventListener('input', function (e) {"
  + "    const o = document.getElementById('saida');"
  + "    const i = document.getElementById('nome');"
  + "    if (o !== null && i !== null) { o.setInnerHTML('valor: ' + i.value); }"
  + "  }); }"
  + "  const p = document.getElementById('par');"
  + "  if (p !== null) { p.addEventListener('mouseover', function (e) {"
  + "    const l = document.getElementById('log');"
  + "    if (l !== null) { l.setInnerHTML('hover'); }"
  + "  }); }"
  + "</script>"
  + "</body></html>";

const doc = parseDocument(html);
runScripts(doc);

// ── 1) por SELETOR: preencher um campo e ler o resultado ──────────────────────
const digitado = doc.type("#nome", "Marcos");
const saidaDepois = doc.textOf("#saida");
const valorLido = doc.valueOfField("#nome");

// ── 2) limpar o campo ─────────────────────────────────────────────────────────
const inp = doc.querySelector("#nome");
const aposClear = inp === null ? "?" : inp.clear();

// ── 3) el.click() direto no nó (sem geometria) ────────────────────────────────
const lnk = doc.querySelector("#lnk");
const hrefPorNo = lnk === null ? "" : lnk.click();

// ── 4) GESTO: mouse simulando usuário (move dispara hover; click navega) ──────
const m = doc.mouse;
m.viewport(VW);
// Acha o link pela GEOMETRIA que o próprio layout reporta, em vez de varrer a
// tela pixel a pixel: uma varredura de 200×600 fazia milhares de hit-tests e,
// sob a suíte paralela, cada um podia recomputar o layout (o GEOM_CACHE guarda um
// documento por vez) — foi o que tornou este arquivo "flaky" na primeira versão.
const elLnk = doc.querySelector("#lnk");
const rectL = elLnk === null ? null : elLnk.getBoundingClientRect(VW);
const xL = rectL === null ? -1 : rectL.x + rectL.width / 2;
const yL = rectL === null ? -1 : rectL.y + rectL.height / 2;
// Um move até o ponto para gerar o hover (é a transição que dispara `mouseover`).
if (xL >= 0) m.move(xL, yL);
const logAposHover = doc.textOf("#log");
const hrefPorGesto = xL < 0 ? "" : m.click(xL, yL);

// ── 5) TECLADO: digitar tecla-a-tecla no campo focado ─────────────────────────
const inp2 = doc.querySelector("#nome");
if (inp2 !== null) inp2.focus();
const kb = doc.keyboard;
kb.typeText("ab");
const aposTeclado = doc.valueOfField("#nome");
kb.press("Backspace");
const aposBackspace = doc.valueOfField("#nome");

// ── 6) existência ─────────────────────────────────────────────────────────────
const temForm = doc.exists("#f");
const temNada = doc.exists("#nao-existe");

describe("automação headless: por seletor", () => {
  test("type() escreve no campo", () => {
    expect(digitado).toBe("Marcos");
  });

  test("o evento input disparou e a página reagiu", () => {
    expect(saidaDepois).toBe("valor: Marcos");
  });

  test("valueOfField lê o valor corrente", () => {
    expect(valorLido).toBe("Marcos");
  });

  test("clear() esvazia o campo", () => {
    expect(aposClear).toBe("");
  });

  test("el.click() num link devolve o href, sem geometria", () => {
    expect(hrefPorNo).toBe("/ok");
  });

  test("exists() distingue o que há do que não há", () => {
    expect(temForm).toBe(true);
    expect(temNada).toBe(false);
  });
});

describe("automação headless: simulando o usuário (gesto)", () => {
  test("mouse.move encontra o link pela geometria", () => {
    expect(xL >= 0).toBe(true);
  });

  test("mover o mouse disparou mouseover na página", () => {
    expect(logAposHover).toBe("hover");
  });

  test("mouse.click faz a sequência completa e navega", () => {
    expect(hrefPorGesto).toBe("/ok");
  });

  test("keyboard.typeText digita tecla a tecla", () => {
    expect(aposTeclado).toBe("ab");
  });

  test("keyboard.press('Backspace') apaga", () => {
    expect(aposBackspace).toBe("a");
  });
});
