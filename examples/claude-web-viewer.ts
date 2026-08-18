// Baixa uma PÁGINA DA WEB e a mostra na janela, pelo motor novo.
//
//   cargo run --release -p rts-host --features ui --example ui_fixture -- \
//       examples/claude-web-viewer.ts
//
// O caminho: `node:https` busca o HTML, os `<link rel=stylesheet>` são buscados
// e embutidos (o `rts-dom` faz a cascata sobre `<style>`, não sobre links), e
// `egui.html` parseia para a árvore retida e a pinta.
//
// # O que isto NÃO faz, e por que importa aqui
//
// Não executa o JavaScript da página. Num site que renderiza no servidor isso é
// quase invisível; num que monta o DOM inteiro no cliente — o WhatsApp Web é o
// caso extremo — o que chega é o SHELL, e o shell é quase vazio de propósito.
// Ver o app exige rodar o JS dele, que é outro problema (o `runScripts` do motor
// antigo; no motor novo o `rts:dom` ainda não foi portado).

import {
  openWindow, pump, isOpen, close, beginFrame, endFrame,
  html, drawText, winWidth,
} from "rts:egui";
import { get } from "node:https";

const HOST = "web.whatsapp.com";
const CAMINHO = "/";

/// Baixa um recurso e devolve o corpo como texto. Síncrono na aparência: o loop
/// de frames só começa depois que a página chegou.
function baixar(host: string, caminho: string): string {
  let corpo = "";
  let pronto = false;
  const req = get({ host: host, path: caminho }, (res: any) => {
    res.on("data", (pedaco: any) => { corpo = corpo + pedaco; });
    res.on("end", () => { pronto = true; });
  });
  req.on("error", (e: any) => { console.log("erro:", e); pronto = true; });
  // espera ativa: este exemplo roda na thread da janela e não tem outro trabalho
  // a fazer antes de a página chegar.
  let voltas = 0;
  while (!pronto && voltas < 2000000) { voltas = voltas + 1; }
  return corpo;
}

console.log("baixando https://" + HOST + CAMINHO);
let fonte = baixar(HOST, CAMINHO);
console.log("recebido:", fonte.length, "bytes");

const win = openWindow("rts-dom — https://" + HOST + CAMINHO, 1100, 780, 0);
if (win <= 0) {
  console.log("não abriu a janela");
} else {
  let frames = 0;
  while (isOpen(win)) {
    pump(win);
    beginFrame(win);
    html(win, fonte);
    drawText(win, "rts-dom · " + HOST + " · " + fonte.length + " bytes · frame " + frames, 0);
    endFrame(win);
    frames = frames + 1;
  }
  close(win);
  console.log("fechou depois de", frames, "frames");
}
