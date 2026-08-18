// A folha externa CASA com esta página? Conta, por classe usada no documento,
// quantas têm regra na folha — a diferença entre "o CSS não foi aplicado" e "o
// CSS é para elementos que só existem depois do JavaScript".
import { readFileSync } from "node:fs";
import { parseHtml, addStylesheet, querySelectorAllCount, querySelectorAllAt,
         getAttribute, computedProperty } from "rts:dom";
const html = readFileSync("wa.html", "utf8") as string;
const css = readFileSync("wa.css", "utf8") as string;
const doc = parseHtml(html);
addStylesheet(doc, css);

const total = querySelectorAllCount(doc, "*");
let comClasse = 0, classesDistintas = 0, naFolha = 0;
const vistas: any = {};
for (let i = 0; i < total; i = i + 1) {
  const n = querySelectorAllAt(doc, "*", i);
  const cls = (getAttribute(doc, n, "class") as string).trim();
  if (cls.length === 0) { continue; }
  comClasse = comClasse + 1;
  for (const c of cls.split(" ")) {
    if (c.length === 0 || vistas[c]) { continue; }
    vistas[c] = true;
    classesDistintas = classesDistintas + 1;
    if (css.indexOf("." + c) >= 0) { naFolha = naFolha + 1; }
  }
}
console.log("elementos:", total, "| com class:", comClasse,
            "| classes distintas:", classesDistintas, "| dessas, na folha:", naFolha);
// e quantos têm alguma cor/fundo computados (prova de cascata a chegar ao nó)
let comCor = 0, comFundo = 0;
for (let i = 0; i < total; i = i + 1) {
  const n = querySelectorAllAt(doc, "*", i);
  if ((computedProperty(doc, n, "color") as string).length > 0) { comCor = comCor + 1; }
  if ((computedProperty(doc, n, "background-color") as string).length > 0) { comFundo = comFundo + 1; }
}
console.log("com color computado:", comCor, "| com background-color:", comFundo);
