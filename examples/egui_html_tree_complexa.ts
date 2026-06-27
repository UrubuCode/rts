import egui from "rts:egui";
import dom from "rts:dom";

// Janela mínima — só pra ter um handle e poder chamar html()/domDump().
class Window {
  __h: number;
  constructor(t: string, w: number, h: number) { this.__h = egui.openWindow(t, w, h, 0); }
  isOpen(): boolean { return egui.isOpen(this.__h) !== 0; }
  pump(): boolean { return egui.pump(this.__h) === 0; }
  beginFrame(): void { egui.beginFrame(this.__h); }
  endFrame(): void { egui.endFrame(this.__h); }
  html(s: string): void { egui.html(this.__h, s); }
  dumpDom(): void { dom.dump(this.__h); }
  close(): void { egui.close(this.__h); }
}

// ── Alocador dinâmico de blocos: o mapa tag → layout vive AQUI, em TS comum ───
// (Demonstra que NÃO precisa da fachada nem de recompilar o Rust.)
// NB: registro no TOP-LEVEL com literais diretos — o motor novo ainda não
// captura variável livre usada como argumento de call dentro de função (#1726),
// então evitamos const-como-argumento e função wrapper.
//   DISPLAY: 0=vertical(block) 1=wrap(inline-flow) 2=horizontal 3=grid
//   PREFIX:  0=none 1=bullet 2=number
//   FLAGS:   MONO=1 PRESERVE_WS=2 HEADING=4 BOLD=8 ITALIC=16
dom.defineBlock("h1", 0, 28, 0, 4);
dom.defineBlock("h2", 0, 22, 0, 4);
dom.defineBlock("h3", 0, 18, 0, 4);
dom.defineBlock("p", 1, 0, 0, 0);
dom.defineBlock("div", 0, 0, 0, 0);
dom.defineBlock("section", 0, 0, 0, 0);
dom.defineBlock("article", 0, 0, 0, 0);
dom.defineBlock("footer", 0, 0, 0, 0);
dom.defineBlock("blockquote", 0, 24, 0, 0);
dom.defineBlock("ul", 0, 16, 0, 0);
dom.defineBlock("ol", 0, 16, 0, 0);
dom.defineBlock("li", 1, 0, 1, 0);
dom.defineBlock("table", 3, 0, 0, 0);
dom.defineBlock("tr", 2, 0, 0, 0);
dom.defineBlock("td", 1, 0, 0, 0);
dom.defineBlock("th", 1, 0, 0, 8);
dom.defineBlock("pre", 0, 0, 0, 3);
dom.defineInline("b", 8);
dom.defineInline("strong", 8);
dom.defineInline("i", 16);
dom.defineInline("em", 16);
dom.defineInline("code", 1);

// HTML com uma árvore PROPOSITALMENTE complexa, pra estressar o parser/render:
//  - aninhamento profundo de blocos (div > div > div)
//  - listas (ul/ol/li) — tags ainda não tratadas como bloco pelo render
//  - tabela (table/tr/td) — idem
//  - inline triplo (b>i>... ) e mistura inline+bloco
//  - atributos (descartados hoje, mas vão importar pro querySelector)
//  - entidades, texto solto entre blocos, tag desconhecida (section/article)
//  - blockquote, pre/code, a (link), span com id/class
const page =
  "<h1>Relatorio</h1>" +
  "<section>" +
    "<h2>1. Visao geral</h2>" +
    "<p>Texto com <b>negrito</b>, <i>italico</i> e <b><i>ambos</i></b> numa frase.</p>" +
    "<div class='card'>" +
      "<div class='card-head'><h3>Card aninhado</h3></div>" +
      "<div class='card-body'>" +
        "<p>Paragrafo dentro de <span id='alvo' class='hl'>span com id</span> e mais texto.</p>" +
        "<blockquote>Uma citacao <i>destacada</i>.</blockquote>" +
      "</div>" +
    "</div>" +
  "</section>" +
  "<section>" +
    "<h2>2. Lista de itens</h2>" +
    "<ul>" +
      "<li>Primeiro item</li>" +
      "<li>Segundo com <b>enfase</b></li>" +
      "<li>Terceiro<ul><li>sub-item a</li><li>sub-item b</li></ul></li>" +
    "</ul>" +
    "<ol>" +
      "<li>passo um</li>" +
      "<li>passo dois</li>" +
    "</ol>" +
  "</section>" +
  "<section>" +
    "<h2>3. Tabela</h2>" +
    "<table>" +
      "<tr><th>Nome</th><th>Valor</th></tr>" +
      "<tr><td>alpha</td><td>10</td></tr>" +
      "<tr><td>beta</td><td>20</td></tr>" +
    "</table>" +
  "</section>" +
  "<article>" +
    "<h2>4. Codigo &amp; escape</h2>" +
    "<pre><code>if (a &lt; b) { return a &amp; b; }</code></pre>" +
    "<p>Veja o <a href='https://x'>link</a> e o fim.</p>" +
  "</article>" +
  "<footer>Rodape do <b>documento</b>.</footer>";

const win = new Window("Tree complexa", 520, 640);
let dumped = false;
while (win.isOpen()) {
  if (!win.pump()) break;
  win.beginFrame();
  win.html(page);
  win.endFrame();
  if (!dumped) { win.dumpDom(); dumped = true; }
}
win.close();
