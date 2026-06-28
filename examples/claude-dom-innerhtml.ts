// innerHTML / outerHTML — manipular o DOM via HTML string (o jeito #1 em apps).
// SET é via MÉTODO setInnerHTML() (o motor RTS não dispara setters de propriedade).
//   target/release/rts.exe run examples/claude-dom-innerhtml.ts
import { io } from "rts";

const d = parseDocument("<div id='app'><p>conteudo inicial</p></div>");
const app = d.querySelector("#app");
if (app !== null) {
  io.print("innerHTML inicial: " + app.innerHTML);

  // SET — parseia o HTML e substitui os filhos (a árvore real muda).
  app.setInnerHTML("<h2>Painel</h2><ul><li>um</li><li>dois</li></ul>");
  io.print("apos setInnerHTML: " + app.innerHTML);

  // querySelector acha os nós novos (parseados de verdade).
  const h2 = d.querySelector("h2");
  if (h2 !== null) io.print("h2 textContent: " + h2.textContent);

  // outerHTML inclui o proprio <div>.
  io.print("outerHTML: " + app.outerHTML);
}
