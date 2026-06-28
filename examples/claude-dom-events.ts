// Eventos do DOM (#1760) — modelo de POLLING + bubbling. addEventListener marca o
// nó; dispatchEvent enfileira com bubbling; o loop consome via pollEvent/Type.
// (O motor não guarda callbacks de fn — os handlers ficam no switch do loop TS.)
//   target/release/rts.exe run examples/claude-dom-events.ts
import dom from "rts:dom";
import { io } from "rts";

const d = dom.parseHtml("<form id='form'><input id='name'><button id='submit'>OK</button></form>");
const form = dom.querySelector(d, "#form");
const submit = dom.querySelector(d, "#submit");

// registra: o form escuta 'submit' (bubbling), o botão escuta 'click'.
dom.addListener(d, form, "submit");
dom.addListener(d, submit, "click");

// simula um clique no botão E um submit no form.
dom.dispatchEvent(d, submit, "click");
dom.dispatchEvent(d, form, "submit");

// o LOOP consome a fila e despacha (aqui um switch por NodeId+tipo).
io.print("=== processando eventos ===");
let ev = dom.pollEvent(d);
while (ev !== -1) {
  const t = dom.pollEventType(d);
  if (ev === submit && t === "click") { io.print("-> botão clicado!"); }
  if (ev === form && t === "submit") { io.print("-> formulário enviado!"); }
  ev = dom.pollEvent(d);
}
io.print("=== fim ===");
