// WebSocket client no RTS — conecta, envia, recebe echo (RFC 6455 sobre TLS).
//   target/release/rts.exe run examples/claude-ws-echo.ts
import ws from "rts:ws";
import { time } from "rts";

const h: i64 = ws.connect("wss://echo.websocket.org/");
if (h === 0) {
  console.log("falha ao conectar");
} else {
  console.log("conectado (handle " + h + ")");
  ws.send(h, "Ola do RTS!");
  // poll pela resposta (o echo server manda um welcome + o echo)
  let got = 0;
  let tries = 0;
  while (got < 2 && tries < 500) {
    const msg = ws.recv(h);
    if (msg.length > 0) {
      console.log("recebido: " + msg);
      got = got + 1;
    }
    time.sleep_ms(10);
    tries = tries + 1;
  }
  ws.close(h);
  console.log("fechado");
}
