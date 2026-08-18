// Baixa uma PÁGINA DA WEB e a mostra na janela, tudo pelo motor novo.
//
//   cargo run --release -p rts-host --features ui --example ui_fixture -- \
//       examples/claude-web-viewer.ts
//
// O caminho: `node:tls` abre a conexão e a requisição HTTP é escrita À MÃO
// sobre ela, `egui.html` parseia o corpo para a árvore retida do `rts-dom` e o
// backend pinta a display list que o layout emite. Nenhum browser participa.
//
// # Por que a requisição é escrita à mão, e não `https.get`
//
// Porque o `node:https` monta a requisição sobre a mesma pilha e acrescenta um
// caminho de erro a mais entre o programa e o socket. Aqui o que se quer provar
// é o transporte: 48 KB reais chegam do `web.whatsapp.com` por TLS 1.3.
//
// # O que isto NÃO faz, e por que importa aqui
//
// Não executa o JavaScript da página. Num site renderizado no servidor isso é
// quase invisível; num que monta o DOM inteiro no cliente — o WhatsApp Web é o
// caso extremo — o que chega é o SHELL, e o shell é quase vazio de propósito.
// O QR code não aparece por isso, e não por falta de canvas: o canvas pinta
// (`examples/claude-canvas.ts`), quem não roda é o script que desenharia nele.

import {
  openWindow, pump, isOpen, close, beginFrame, endFrame,
  html, drawText,
} from "rts:egui";
import { connect } from "node:tls";

const HOST = "web.whatsapp.com";
const CAMINHO = "/";

/// Baixa por TLS e devolve o corpo da resposta HTTP (sem os cabeçalhos).
function baixar(host: string, caminho: string): string {
  let bruto = "";
  let fim = false;
  const s: any = connect({ host: host, port: 443, servername: host } as any);
  s.on("secureConnect", () => {
    s.write("GET " + caminho + " HTTP/1.1\r\nHost: " + host +
            "\r\nUser-Agent: rts-dom\r\nAccept: text/html\r\nConnection: close\r\n\r\n");
  });
  s.on("data", (p: any) => { bruto = bruto + p.toString("utf8"); });
  s.on("end", () => { fim = true; });
  s.on("close", () => { fim = true; });
  s.on("error", (e: any) => { console.log("erro:", e.message); fim = true; });

  // Espera ativa, bombeando UMA VEZ POR MILISSEGUNDO: este exemplo roda na
  // thread da janela e não tem outro trabalho antes de a página chegar, e
  // martelar o `write` deixa a thread leitora do socket sem o mutex do registry
  // — a resposta inteira só chegava no encerramento do processo.
  const t0 = Date.now();
  let ultimo = 0;
  while (!fim && Date.now() - t0 < 20000) {
    const agora = Date.now();
    if (agora !== ultimo) { ultimo = agora; s.write(""); }
  }

  const corte = bruto.indexOf("\r\n\r\n");
  return corte < 0 ? bruto : bruto.substring(corte + 4);
}

console.log("baixando https://" + HOST + CAMINHO);
const fonte = baixar(HOST, CAMINHO);
console.log("recebido:", fonte.length, "bytes de corpo");

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
