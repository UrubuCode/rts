// React 18 real, dos bundles UMD do CDN, a montar na nossa janela.
//
// A diferenca para o headless e o LOOP: o React 18 e concurrent — `render`
// AGENDA e o trabalho corre depois, fatia a fatia. Sem um loop vivo ninguem o
// bombeia, e o `#root` fica vazio sem um erro. Aqui o frame do host bombeia.
import { openWindow, pump, isOpen, close, beginFrame, endFrame, render, drawText } from "rts:egui";
import { readFileSync } from "node:fs";

const doc = parseDocument(readFileSync("react-app.html", "utf8") as string);
console.log("scripts que correram:", runScripts(doc));
// LOGO a seguir aos scripts, antes de qualquer janela: o React montou?
// Um `await` SUSPENDE o programa e devolve o controlo ao host, que corre o
// loop ate a promessa resolver. E isso — nao `run_event_loop` chamado de
// dentro — que faz o trabalho agendado avancar.
let voltas = 0;
while (voltas < 30) { await new Promise(function (r: any) { setTimeout(r, 0); }); voltas = voltas + 1; }
const antes = doc.getElementById("root");
console.log("filhos do #root ANTES da janela:", antes === null ? "?" : antes.children.length);

const win = openWindow("rts-dom — React 18 a correr", 900, 600, 0);
let frames = 0;
while (isOpen(win)) {
  pump(win);
  // O trabalho que o React agendou: uma volta do loop do motor e a fila de
  // timers do documento, por frame. E o que um browser faz entre pinturas.
  pumpTimerCallbacks(doc);
  beginFrame(win);
  render(win, doc._dom);
  const raiz = doc.getElementById("root");
  const n = raiz === null ? 0 : raiz.children.length;
  drawText(win, "rts-dom · React 18 · filhos do #root: " + n + " · frame " + frames, 0);
  endFrame(win);
  frames = frames + 1;
}
close(win);
const raiz = doc.getElementById("root");
console.log("no fim, filhos do #root:", raiz === null ? "?" : raiz.children.length);
console.log("texto:", raiz === null ? "" : (raiz.textContent || "").substring(0, 120));
