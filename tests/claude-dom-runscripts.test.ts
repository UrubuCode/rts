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
  "</script>" +
  "<script>" +
  "const t3 = document.getElementById('t');" +
  "if (t3 !== null) { t3.textContent = 'via setter'; }" +
  "</script>";

const doc = parseDocument(html);
const ran = runScripts(doc);

const t = doc.getElementById("t");
const textoDepois = t === null ? "" : t.textContent;
const attrDepois = t === null ? "" : t.getAttribute("data-seq");
const lis = doc.querySelectorAll("#list li");
const nLis = lis.length;
const ultimo = nLis === 3 ? lis[2].textContent : "";

// <script src="data:...;base64,..."> — o WhatsApp/Meta embutem quase todo o JS
// assim. base64 de: const el=document.getElementById('d'); if(el!==null){el.setInnerHTML('data-uri ok');}
const b64 = "Y29uc3QgZWw9ZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoJ2QnKTsgaWYoZWwhPT1udWxsKXtlbC5zZXRJbm5lckhUTUwoJ2RhdGEtdXJpIG9rJyk7fQ==";
const htmlData = "<div id='d'>x</div><script src='data:application/x-javascript;base64," + b64 + "'></script>";
const docData = parseDocument(htmlData);
const ranData = runScripts(docData);
const dEl = docData.getElementById("d");
const dataResult = dEl === null ? "" : dEl.textContent;

describe("runScripts — bloco <script> da página", () => {
  test("scripts válidos rodam; o quebrado é isolado", () => {
    expect(ran).toBe(3);
  });
  test("script muta o DOM via SETTER de propriedade (como no browser)", () => {
    expect(textoDepois).toBe("via setter");
  });
  test("script cria elementos dinamicamente", () => {
    expect(nLis).toBe(3);
    expect(ultimo).toBe("item 2");
  });
  test("script após o quebrado ainda roda", () => {
    expect(attrDepois).toBe("terceiro");
  });
  test("script src=data:base64 é decodificado e executado", () => {
    expect(ranData).toBe(1);
    expect(dataResult).toBe("data-uri ok");
  });
});
