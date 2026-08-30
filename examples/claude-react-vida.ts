// O Jogo da Vida de Conway, escrito em React, a correr sozinho na nossa janela.
//
// Nao precisa de cliques — e por isso que serve de teste: o que ele exercita e
// `useEffect`, `setInterval`, atualizacao funcional de estado e RE-RENDER
// continuo. A grelha inteira muda a cada geracao, e o React reconcilia.
import { openWindow, pump, isOpen, close, beginFrame, endFrame, render, drawText } from "rts:egui";
import { readFileSync } from "node:fs";

const doc = parseDocument(readFileSync("react-vida.html", "utf8") as string);
console.log("scripts que correram:", runScripts(doc));

// Deixar o React montar antes de abrir a janela: um `await` devolve o controlo
// ao host, que corre o loop — `run_event_loop` chamado de dentro nao o faz.
let v = 0;
while (v < 30) { await new Promise(function (r: any) { setTimeout(r, 0); }); v = v + 1; }
const raiz = doc.getElementById("root");
console.log("montou? filhos do #root:", raiz === null ? "?" : raiz.children.length);

const win = openWindow("rts-dom — Jogo da Vida em React", 700, 640, 0);
let frames = 0;
while (isOpen(win)) {
  pump(win);
  // Uma volta do loop desta pagina por frame: e daqui que o `setInterval` do
  // jogo sai da fila e a geracao seguinte e calculada.
  pumpTimerCallbacks(doc);
  beginFrame(win);
  render(win, doc._dom);
  const p = raiz === null ? null : raiz.querySelector("p");
  drawText(win, "rts-dom · React 18 · " + (p === null ? "" : p.textContent) + " · frame " + frames, 0);
  endFrame(win);
  frames = frames + 1;
}
close(win);
const p = raiz === null ? null : raiz.querySelector("p");
console.log("no fim:", p === null ? "?" : p.textContent, "em", frames, "frames");
