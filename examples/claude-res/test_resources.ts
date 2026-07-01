import dom from "rts:dom";
// Testa o carregamento de recursos externos do DOM (CSS via <link> + @import; <script src>).
// Usa os primitivos dom.* diretos (a fachada Element.method() sobre querySelector baila
// no motor — limite conhecido). baseUrl = caminho do "documento".
const html =
  '<html><head>' +
  '<link rel="stylesheet" href="./base.css">' +
  '<script src="./hello.js"></script>' +
  '</head><body><h1>titulo</h1><p>paragrafo</p></body></html>';

const doc = parseDocument(html);
const h = doc._dom;
const base = "examples/claude-res/index.html";
const n = loadResources(doc, base);
console.log("recursos carregados: " + n);

// h1: color/font-size vieram do base.css; p: color veio do @import palette.css.
const h1 = dom.getByTagAt(h, "h1", 0);
const p = dom.getByTagAt(h, "p", 0);
console.log("h1 color = " + dom.computedProperty(h, h1, "color"));
console.log("h1 size  = " + dom.computedProperty(h, h1, "font-size"));
console.log("p  color = " + dom.computedProperty(h, p, "color") + "  (via @import)");

// <script src> materializado como texto do nó (carregar != executar).
const sc = dom.getByTagAt(h, "script", 0);
console.log("script texto = " + dom.getText(h, sc));
