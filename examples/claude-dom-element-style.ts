// element.style por NOME CSS + getComputedStyle — a API de estilo do browser sobre
// o motor CSS nativo. Como o motor não tem objeto-proxy/setter, é via MÉTODOS.
//   target/release/rts.exe run examples/claude-dom-element-style.ts
import { io } from "rts";

const d = parseDocument(
  "<style>.box{color:#333333;font-size:16px;padding:8px}</style>" +
  "<div class='box' id='card' style='margin: 10px'>conteúdo</div>");

const card = d.querySelector("#card");
if (card !== null) {
  // getComputedStyle — valor após a cascade (<style> + inline), formato do browser.
  io.print("computed color: " + card.getComputedProp("color"));        // rgb(51, 51, 51)
  io.print("computed font-size: " + card.getComputedProp("fontSize")); // 16px (camelCase ok)
  io.print("computed padding-top: " + card.getComputedProp("paddingTop"));

  // style.setProperty — escreve no style="" inline (preserva as outras).
  card.setStyleProp("backgroundColor", "navy");
  card.setStyleProp("border-width", "2px");
  io.print("cssText apos sets: " + card.cssText);

  // o computed reflete o inline novo.
  io.print("computed background: " + card.getComputedProp("backgroundColor"));

  // removeProperty.
  card.removeStyleProp("margin");
  io.print("cssText apos remove margin: " + card.cssText);
}
