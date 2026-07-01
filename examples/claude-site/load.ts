import dom from "rts:dom";
import fs from "rts:fs";

// 1) Lê o HTML do disco (como um browser abrindo um arquivo).
const html = fs.read_text("examples/claude-site/index.html");
console.log("HTML lido: " + html.length + " bytes");

// 2) Parseia → DOM.
const doc = parseDocument(html);
const h = doc._dom;

// 3) Carrega recursos externos (<link rel=stylesheet>, @import, <script src>).
const n = loadResources(doc, "examples/claude-site/index.html");
console.log("recursos externos carregados: " + n);

// 4) Prova a cascade: estilos vindos de theme.css, @import reset.css e <style> inline.
function show(sel: string, tag: string, idx: number, prop: string): void {
  const node = dom.getByTagAt(h, tag, idx);
  console.log("  " + sel + " { " + prop + ": " + dom.computedProperty(h, node, prop) + " }");
}
console.log("--- estilos computados (cascade real) ---");
show("body", "body", 0, "background");        // reset.css via @import
show("body", "body", 0, "font-size");
show(".hero h1", "h1", 0, "font-size");        // theme.css
show(".hero h1", "h1", 0, "color");
show(".hero p", "p", 0, "font-size");          // <style> inline sobrepõe
show(".card h2 (1)", "h2", 0, "color");        // theme.css
show(".card (1)", "div", 0, "background");
show("#footer", "footer", 0, "background");

// 5) Estrutura do DOM (devtools-like).
console.log("--- contagem de elementos ---");
console.log("  divs (.card): " + dom.getByTagCount(h, "div"));
console.log("  links <a>:    " + dom.getByTagCount(h, "a"));
console.log("  h2:           " + dom.getByTagCount(h, "h2"));
