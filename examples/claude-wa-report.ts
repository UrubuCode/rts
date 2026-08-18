// O que o `rts-dom` FEZ com a página real do WhatsApp Web — sem janela.
//
//   target/release/examples/run_fixture.exe examples/claude-wa-report.ts
//
// Lê `wa.html`/`wa.css` do disco (gravados por `claude-wa-dump.ts`) e responde
// pelo `rts:dom`: quantos nós, o que a cascata computou nos elementos que a
// página mostra, e onde o layout os pôs. É o instrumento que substitui olhar
// para a janela e adivinhar — uma cor errada e uma caixa de largura zero são
// coisas diferentes e a tela mostra as duas como "nada aparece".
import { readFileSync } from "node:fs";
import {
  parseHtml, rootId, addStylesheet, nodeCount,
  querySelector, querySelectorAllCount, querySelectorAllAt,
  getText, tagName, computedProperty, boundingRect, getAttribute,
} from "rts:dom";

const fonte = readFileSync("wa.html", "utf8") as string;
const css = readFileSync("wa.css", "utf8") as string;

const doc = parseHtml(fonte);
addStylesheet(doc, css);
console.log("nós:", nodeCount(doc), "| html:", fonte.length, "| css:", css.length);

function relatar(sel: string): void {
  const n = querySelector(doc, sel);
  if (n < 0) { console.log(sel, "→ não casou"); return; }
  const x = boundingRect(doc, n, 0), y = boundingRect(doc, n, 1);
  const w = boundingRect(doc, n, 2), h = boundingRect(doc, n, 3);
  const texto = (getText(doc, n) as string).trim().substring(0, 40);
  console.log(
    sel + " <" + tagName(doc, n) + ">",
    "rect=" + x + "," + y + " " + w + "x" + h,
    "| bg=" + computedProperty(doc, n, "background-color"),
    "| cor=" + computedProperty(doc, n, "color"),
    "| display=" + computedProperty(doc, n, "display"),
    texto.length > 0 ? "| texto=" + JSON.stringify(texto) : "",
  );
}

for (const sel of ["html", "body", "#app", ".landing-wrapper", ".landing-header",
                   ".landing-title", ".landing-main", "h1", "h2", "canvas", "img", "div"]) {
  relatar(sel);
}

// Quantos elementos a página tem, e quantos deles o layout deu tamanho.
const total = querySelectorAllCount(doc, "*");
let comArea = 0;
let semArea = 0;
for (let i = 0; i < total; i = i + 1) {
  const n = querySelectorAllAt(doc, "*", i);
  const w = boundingRect(doc, n, 2), h = boundingRect(doc, n, 3);
  if (w > 0 && h > 0) { comArea = comArea + 1; } else { semArea = semArea + 1; }
}
console.log("elementos:", total, "| com área:", comArea, "| sem área:", semArea);
