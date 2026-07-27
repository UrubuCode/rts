import { describe, test, expect } from "rts:test";

// CONTROLES DE FORMULÁRIO — o que um site real usa.
//
// Antes disto, `value` lia só o atributo `value=`, então `<textarea>` (cujo valor
// é o texto filho) e `<select>` (cuja seleção vem da `<option>`) devolviam string
// vazia; checkbox/radio não tinham estado nenhum; e clicar num botão de submit
// não submetia. Cada um desses é um formulário que não funciona.

const html = "<html><body>"
  + "<form id='f' action='/enviar' method='post'>"
  + "<input id='txt' type='text' value='ola' />"
  + "<input id='chk' type='checkbox' checked />"
  + "<input id='chk2' type='checkbox' />"
  + "<input id='r1' type='radio' name='cor' value='azul' checked />"
  + "<input id='r2' type='radio' name='cor' value='vermelho' />"
  + "<select id='sel'><option value='a'>A</option><option value='b' selected>B</option></select>"
  + "<select id='sel2'><option value='x'>X</option><option value='y'>Y</option></select>"
  + "<select id='sel3'><option>texto puro</option></select>"
  + "<textarea id='ta'>texto longo</textarea>"
  + "<button id='btn' type='submit'>enviar</button>"
  + "</form>"
  + "<form id='f2' action='/cancelado'>"
  + "<button id='btn2' type='submit'>enviar 2</button>"
  + "</form>"
  + "<script>"
  + "  const f2 = document.getElementById('f2');"
  + "  if (f2 !== null) { f2.addEventListener('submit', function (e) { e.preventDefault(); }); }"
  + "  const c = document.getElementById('chk2');"
  + "  if (c !== null) { c.addEventListener('change', function (e) {"
  + "    const o = document.getElementById('txt');"
  + "    if (o !== null) { o.setAttribute('data-mudou', 'sim'); }"
  + "  }); }"
  + "</script>"
  + "</body></html>";

const doc = parseDocument(html);
runScripts(doc);

// ── valores iniciais, cada um com sua fonte de verdade ───────────────────────
const vTxt = doc.valueOfField("#txt");        // do atributo value=
const vTa = doc.valueOfField("#ta");          // do texto filho
const vSel = doc.valueOfField("#sel");        // da <option selected>
const vSel2 = doc.valueOfField("#sel2");      // sem selected → a PRIMEIRA
const vSel3 = doc.valueOfField("#sel3");      // option sem value= → seu texto

// ── checkbox: estado inicial vem do atributo ─────────────────────────────────
const chk = doc.querySelector("#chk");
const chk2 = doc.querySelector("#chk2");
const chkInicial = chk === null ? false : chk.checked;
const chk2Inicial = chk2 === null ? true : chk2.checked;

// ── clicar num checkbox ALTERNA (ação default) e dispara change ──────────────
if (chk2 !== null) chk2.click();
const chk2Depois = chk2 === null ? false : chk2.checked;
const mudouAttr = doc.querySelector("#txt");
const disparouChange = mudouAttr === null ? "" : mudouAttr.getAttribute("data-mudou");

// clicar de novo desmarca
if (chk2 !== null) chk2.click();
const chk2Terceiro = chk2 === null ? true : chk2.checked;

// ── radio: marcar um LIMPA os irmãos de mesmo name ───────────────────────────
const r1 = doc.querySelector("#r1");
const r2 = doc.querySelector("#r2");
const r1Antes = r1 === null ? false : r1.checked;
if (r2 !== null) r2.click();
const r1Depois = r1 === null ? true : r1.checked;
const r2Depois = r2 === null ? false : r2.checked;
// clicar num radio JÁ marcado não desmarca (regra do HTML)
if (r2 !== null) r2.click();
const r2Ainda = r2 === null ? false : r2.checked;

// ── submit: o botão submete o form e devolve o action ────────────────────────
const btn = doc.querySelector("#btn");
const acao = btn === null ? "" : btn.click();

// ── preventDefault no submit cancela ─────────────────────────────────────────
const btn2 = doc.querySelector("#btn2");
const acao2 = btn2 === null ? "x" : btn2.click();

describe("controles de formulário: valor", () => {
  test("input lê o atributo value=", () => {
    expect(vTxt).toBe("ola");
  });

  test("textarea lê o TEXTO filho", () => {
    expect(vTa).toBe("texto longo");
  });

  test("select lê a <option selected>", () => {
    expect(vSel).toBe("b");
  });

  test("select sem selected usa a primeira option", () => {
    expect(vSel2).toBe("x");
  });

  test("option sem value= vale pelo seu texto", () => {
    expect(vSel3).toBe("texto puro");
  });
});

describe("controles de formulário: checkbox e radio", () => {
  test("checked inicial vem do atributo", () => {
    expect(chkInicial).toBe(true);
    expect(chk2Inicial).toBe(false);
  });

  test("clicar num checkbox alterna", () => {
    expect(chk2Depois).toBe(true);
  });

  test("alternar dispara o evento change", () => {
    expect(disparouChange).toBe("sim");
  });

  test("clicar de novo desmarca", () => {
    expect(chk2Terceiro).toBe(false);
  });

  test("radio começa marcado pelo atributo", () => {
    expect(r1Antes).toBe(true);
  });

  test("marcar um radio limpa o irmão de mesmo name", () => {
    expect(r2Depois).toBe(true);
    expect(r1Depois).toBe(false);
  });

  test("clicar num radio já marcado não desmarca", () => {
    expect(r2Ainda).toBe(true);
  });
});

describe("controles de formulário: submit", () => {
  test("botão submit devolve o action do form", () => {
    expect(acao).toBe("/enviar");
  });

  test("preventDefault no submit cancela a ação default", () => {
    expect(acao2).toBe("");
  });
});
