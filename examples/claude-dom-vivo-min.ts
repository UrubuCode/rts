// O caminho `render(win, doc)` com uma pagina minima: parseia, corre o script,
// e pinta a ARVORE (nao o texto), que e a unica forma de se ver o que o
// `<script>` fez.
import { openWindow, pump, isOpen, close, beginFrame, endFrame, render, drawText } from "rts:egui";
const fonte = "<style>body{font-family:sans-serif} h1{color:#0a0}</style>" +
  "<h1 id='t'>antes do script</h1><p id='p'>-</p>" +
  "<script>" +
  "  var alvo = document.getElementById('t');" +
  "  if (alvo !== null) { alvo.setInnerHTML('DEPOIS do script'); }" +
  "  var p = document.getElementById('p');" +
  "  if (p !== null) { p.setInnerHTML('o JS da pagina correu'); }" +
  "</script>";
const doc = parseDocument(fonte);
console.log("scripts:", runScripts(doc));
const t = doc.getElementById("t");
console.log("titulo depois do script:", t === null ? "(sem no)" : t.textContent);
console.log("handle do documento:", doc._dom);
const win = openWindow("rts-dom — pagina minima", 700, 400, 0);
let frames = 0;
while (isOpen(win)) {
  pump(win); beginFrame(win); render(win, doc._dom);
  drawText(win, "frame " + frames, 0);
  endFrame(win); frames = frames + 1;
}
close(win);
