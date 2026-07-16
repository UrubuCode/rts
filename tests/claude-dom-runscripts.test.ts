import { describe, test, expect } from "rts:test";

// Bloco <script> da página: runScripts compila cada <script> in-process (new
// Function) e roda com `document` apontando pro MESMO DOM (store thread_local).
// Pré-computado no top-level (regra do projeto: chamadas de método dentro de
// test() podem perder handle pro GC).

const html = "<div id='app'><h1 id='t'>antes</h1><ul id='list'></ul></div>" +
  "<script>" +
  "const el = document.getElementById('t');" +
  "if (el !== null) { el.setInnerHTML('mudado'); }" +
  "let i = 0;" +
  "while (i < 3) {" +
  "  const li = document.createElement('li');" +
  "  li.setInnerHTML('item ' + i);" +
  "  const list = document.getElementById('list');" +
  "  if (list !== null) { list.appendChild(li); }" +
  "  i = i + 1;" +
  "}" +
  "</script>" +
  "<script>tem erro de sintaxe {{{</script>" +
  "<script>" +
  "const t2 = document.getElementById('t');" +
  "if (t2 !== null) { t2.setAttribute('data-seq', 'terceiro'); }" +
  "</script>";

const doc = parseDocument(html);
const ran = runScripts(doc);

const t = doc.getElementById("t");
const textoDepois = t === null ? "" : t.textContent;
const attrDepois = t === null ? "" : t.getAttribute("data-seq");
const lis = doc.querySelectorAll("#list li");
const nLis = lis.length;
const ultimo = nLis === 3 ? lis[2].textContent : "";

describe("runScripts — bloco <script> da página", () => {
  test("scripts válidos rodam; o quebrado é isolado", () => {
    expect(ran).toBe(2);
  });
  test("script muta o DOM via método", () => {
    expect(textoDepois).toBe("mudado");
  });
  test("script cria elementos dinamicamente", () => {
    expect(nLis).toBe(3);
    expect(ultimo).toBe("item 2");
  });
  test("script após o quebrado ainda roda", () => {
    expect(attrDepois).toBe("terceiro");
  });
});
