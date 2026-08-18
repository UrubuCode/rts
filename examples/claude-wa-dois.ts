// A MESMA página, duas identidades — a prova de que a cascata funciona e de que
// o que falta na versão do app é o JavaScript, não o CSS.
//
// Com o UA `rts-dom` o servidor devolve a landing "Browser not supported", que
// é HTML servido pronto: tem `<style>` inline e classes que casam. Com os
// cabeçalhos de um Chrome devolve o app real, cujo HTML é um shell — o DOM e a
// folha de layout nascem do JavaScript.
import { readFileSync } from "node:fs";
import { parseHtml, addStylesheet, querySelectorAllCount, querySelectorAllAt,
         getAttribute, computedProperty, boundingRect } from "rts:dom";

function medir(nome: string, ficheiro: string, folha: string): void {
  const doc = parseHtml(readFileSync(ficheiro, "utf8") as string);
  if (folha.length > 0) { addStylesheet(doc, readFileSync(folha, "utf8") as string); }
  const total = querySelectorAllCount(doc, "*");
  let comArea = 0, comFundo = 0, comClasse = 0, pintados = 0;
  for (let i = 0; i < total; i = i + 1) {
    const n = querySelectorAllAt(doc, "*", i);
    if (boundingRect(doc, n, 2) > 0 && boundingRect(doc, n, 3) > 0) { comArea = comArea + 1; }
    if ((computedProperty(doc, n, "background-color") as string).length > 0) { comFundo = comFundo + 1; }
    if ((getAttribute(doc, n, "class") as string).length > 0) { comClasse = comClasse + 1; }
    if ((computedProperty(doc, n, "display") as string).length > 0) { pintados = pintados + 1; }
  }
  console.log(nome, "→ elementos:", total, "| com class:", comClasse,
              "| com área:", comArea, "| com background:", comFundo,
              "| com display da cascata:", pintados);
}

medir("wikipedia (servida pronta)", "pagina.html", "pagina.css");
medir("whatsapp app (shell + JS)  ", "wa-app.html", "wa-app.css");
