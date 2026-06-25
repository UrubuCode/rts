// Testa a fachada DOM REAL (document/Element, API do browser) — vinda do prelude.
// SEM import: document/Element/parseDocument são globais do prelude DOM_TS.

const doc = parseDocument(
  "<div id='app'><h1 class='title'>Ola</h1><ul><li class='item'>A</li><li class='item'>B</li></ul></div>"
);

// querySelector + textContent getter (API real do browser)
const h1 = doc.querySelector(".title");
if (h1 === null) {
  console.log("FALHOU: h1 null");
} else {
  console.log("h1.textContent: " + h1.textContent);
  console.log("h1.tagName: " + h1.tagName);
  // textContent SETTER (el.textContent = x — igual browser)
  h1.textContent = "Mudado via setter";
  console.log("apos setter: " + h1.textContent);
}

// getElementById
const app = doc.getElementById("app");
if (app !== null) {
  console.log("app.tagName: " + app.tagName);
  // setAttribute + getAttribute
  app.setAttribute("data-x", "42");
  console.log("getAttribute(data-x): " + app.getAttribute("data-x"));
  console.log("hasAttribute(nope): " + app.hasAttribute("nope"));
}

// querySelectorAll → array, com for-of e .length
const items = doc.querySelectorAll(".item");
console.log("items.length: " + items.length);
let texts = "";
for (const it of items) {
  texts = texts + it.textContent + " ";
}
console.log("items texts: " + texts);

// createElement + appendChild
const ul = doc.querySelector("ul");
if (ul !== null) {
  const li = doc.createElement("li");
  li.textContent = "C (criado)";
  li.className = "item";
  ul.appendChild(li);
  const after = doc.querySelectorAll(".item");
  console.log("apos append, items: " + after.length);
}

console.log("FACHADA DOM REAL OK");
