// O namespace `rts:dom` no motor NOVO: parse, consulta, mutação e geometria de
// um documento — sem abrir janela.
import {
  parseHtml, free, rootId, querySelector, querySelectorAllCount, querySelectorAllAt,
  getText, setText, getAttribute, setAttribute, createElement, appendChild,
  tagName, childCount, computedProperty, boundingRect, dump, nodeCount,
} from "rts:dom";

const doc = parseHtml("<html><body><main id='r'><p class='t'>um</p><p class='t'>dois</p></main></body></html>");
console.log("documento:", doc, "| nós:", nodeCount(doc), "| raiz:", rootId(doc));

const main = querySelector(doc, "#r");
console.log("#r é", tagName(doc, main), "com", childCount(doc, main), "filhos");

const n = querySelectorAllCount(doc, ".t");
console.log("querySelectorAll('.t') →", n);
let i = 0;
while (i < n) {
  const el = querySelectorAllAt(doc, ".t", i);
  console.log("  [" + i + "]", tagName(doc, el), "=", getText(doc, el));
  i = i + 1;
}

const primeiro = querySelector(doc, ".t");
setText(doc, primeiro, "TROCADO");
setAttribute(doc, primeiro, "data-x", "42");
console.log("depois de mutar:", getText(doc, primeiro), "| data-x =", getAttribute(doc, primeiro, "data-x"));

const novo = createElement(doc, "span");
setText(doc, novo, "criado por codigo");
appendChild(doc, main, novo);
console.log("filhos agora:", childCount(doc, main), "| ultimo:", getText(doc, novo));

console.log("cor computada do p:", computedProperty(doc, primeiro, "color"));
console.log("caixa do #r: x", boundingRect(doc, main, 0), "y", boundingRect(doc, main, 1),
            "w", boundingRect(doc, main, 2), "h", boundingRect(doc, main, 3));

console.log("--- árvore ---");
console.log(dump(doc));
free(doc);
