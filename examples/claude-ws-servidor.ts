// Um servidor WebSocket, com o pacote `ws`.
//
//   cargo run -q -p rts-host --example run_fixture examples/claude-ws-servidor.ts
//
// e, de fora, qualquer cliente WebSocket:
//
//   python -c "import asyncio,websockets; \
//     asyncio.run((lambda: (lambda ws: None))())"   # ver docs/reference/node/ws.md
//
// É `ws` e não `node:ws` de propósito: no Node um servidor WebSocket não é da
// plataforma — vem do pacote npm — e inventar `node:ws` daria um nome que
// nenhum programa portado procuraria.

import { WebSocketServer } from "ws";

const wss = new WebSocketServer({ port: 7788 });

// Um servidor à ESCUTA não segura o programa aberto: ele responde
// `Pending::Blocked`, que o laço bombeia mas não espera — a decisão que permite
// um teste terminar em vez de rodar para sempre. Então quem só escuta precisa
// de algo que o segure, e o timer é isso.
//
// Uma CONEXÃO aberta é diferente e segura sozinha, como no Node.
setTimeout(() => println("[servidor] tempo esgotado, saindo"), 60000);

wss.on("listening", () => println("[servidor] escutando em ws://127.0.0.1:7788"));

wss.on("connection", (ws: any, req: any) => {
  println("[servidor] conectou — url=" + req.url + " host=" + req.headers.host);

  ws.on("message", (dados: any, binario: any) => {
    // `dados` é uma string para um frame TEXT e um `Buffer` para um BINARY, que
    // é o que o `ws` entrega. `binario` diz qual dos dois, sem precisar
    // adivinhar pelo conteúdo.
    println("[servidor] recebeu (binario=" + binario + "): " + dados.toString());
    ws.send("eco: " + dados.toString());
  });

  ws.on("error", (erro: any) => println("[servidor] erro: " + erro.message));

  ws.on("close", (code: any, motivo: any) => {
    println("[servidor] fechou — code=" + code + " motivo=" + motivo);
    wss.close();
  });
});
