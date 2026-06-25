// E2E headless: usa rts:dom SEM abrir nenhuma janela. Prova o desacoplamento.
import dom from "rts:dom";

const NONE = -1;

// 1) parseHtml → handle de DOM avulso (sem janela!)
const d = dom.parseHtml("<div id='alvo'>antes</div><p class='msg'>texto</p>");
console.log("dom handle >= 1: " + (d >= 1));

// 2) querySelector por #id e .classe
const alvo = dom.querySelector(d, "#alvo");
const msg = dom.querySelector(d, ".msg");
console.log("achou #alvo: " + (alvo !== NONE));
console.log("achou .msg: " + (msg !== NONE));

// 3) mutação: setText + setAttr
if (alvo !== NONE) dom.setText(d, alvo, "DEPOIS via rts:dom");
if (msg !== NONE) dom.setAttr(d, msg, "class", "msg destaque");

// 4) createElement + appendChild (montar nó novo)
const root = dom.rootId(d);
const li = dom.createElement(d, "li");
dom.setText(d, li, "item criado headless");
dom.appendChild(d, root, li);

// 5) dump da árvore mutada (prova tudo, sem render)
dom.dump(d);

// 6) free
dom.free(d);
console.log("headless OK");
