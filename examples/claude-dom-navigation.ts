// Testa os métodos de DOM CORE novos (conformidade com a definição da Mozilla):
// navegação (parentNode/firstChild/lastChild/nextSibling/previousSibling),
// childNodes, createTextNode, insertBefore, classList, nodeType/nodeName.
// Headless (sem janela) — só console.log. SEM import (document/Element/
// parseDocument vêm do prelude DOM_TS).
//   target/release/rts.exe run examples/claude-dom-navigation.ts

const doc = parseDocument(
  "<ul id='lista'><li class='item'>A</li><li class='item sel'>B</li><li class='item'>C</li></ul>"
);

const ul = doc.getElementById("lista");
if (ul === null) {
  console.log("FALHOU: ul null");
} else {
  // ── navegação ──────────────────────────────────────────────────────────────
  const first = ul.firstChild;
  const last = ul.lastChild;
  if (first !== null) console.log("firstChild.textContent: " + first.textContent); // A
  if (last !== null) console.log("lastChild.textContent: " + last.textContent); // C

  if (first !== null) {
    const second = first.nextSibling;
    if (second !== null) {
      console.log("first.nextSibling: " + second.textContent); // B
      const back = second.previousSibling;
      if (back !== null) console.log("second.previousSibling: " + back.textContent); // A
      const parent = second.parentNode;
      if (parent !== null) console.log("second.parentNode.tagName: " + parent.tagName); // UL
    }
  }

  // ── nodeType / nodeName ──────────────────────────────────────────────────────
  console.log("ul.nodeType: " + ul.nodeType); // 1 (Element)
  console.log("ul.nodeName: " + ul.nodeName); // ul
  if (first !== null) {
    const txt = first.firstChild; // o nó de TEXTO dentro do <li>
    if (txt !== null) {
      console.log("textNode.nodeType: " + txt.nodeType); // 3 (Text)
      console.log("textNode.nodeName: " + txt.nodeName); // #text
    }
  }

  // ── childNodes (inclui texto) vs children (só elementos) ─────────────────────
  console.log("ul.children.length: " + ul.children.length); // 3 (li)

  // ── createElement + createTextNode + insertBefore ────────────────────────────
  const novo = doc.createElement("li");
  const t = doc.createTextNode("NOVO");
  novo.appendChild(t);
  ul.insertBefore(novo, first); // novo vira o primeiro
  const nf = ul.firstChild;
  if (nf !== null) console.log("apos insertBefore, firstChild: " + nf.textContent); // NOVO

  // ── classList ────────────────────────────────────────────────────────────────
  const li2 = doc.querySelector(".sel"); // o <li> B (tem classe 'sel')
  if (li2 !== null) {
    console.log("li2.classListContains(sel): " + li2.classListContains("sel")); // true
    li2.classListRemove("sel");
    console.log("apos remove, contains(sel): " + li2.classListContains("sel")); // false
    li2.classListAdd("destaque");
    console.log("apos add, className: " + li2.className); // "item destaque"
    const novoEstado = li2.classListToggle("sel");
    console.log("toggle(sel) -> " + novoEstado + ", className: " + li2.className); // true, "item destaque sel"
  }
}

console.log("=== fim ===");
