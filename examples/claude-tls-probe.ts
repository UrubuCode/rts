// TLS cru contra o WhatsApp Web: a requisição HTTP escrita na mão sobre TLS.
import { connect } from "node:tls";

let corpo = "";
let fim = false;
const s: any = connect({ host: "web.whatsapp.com", port: 443, servername: "web.whatsapp.com" } as any);
s.on("secureConnect", () => {
  s.write("GET / HTTP/1.1\r\nHost: web.whatsapp.com\r\nUser-Agent: rts\r\nConnection: close\r\n\r\n");
});
s.on("data", (p: any) => { corpo = corpo + p.toString("utf8"); });
s.on("end", () => { fim = true; });
s.on("close", () => { fim = true; });
s.on("error", (e: any) => { console.log("erro:", e.message); fim = true; });

// Espera por TEMPO e não por contagem de voltas: 400 mil voltas de `write("")`
// passam em menos de um RTT, e o laço terminava antes de a resposta existir.
const t0 = Date.now();
let v = 0;
// Bombeia UMA VEZ POR MILISSEGUNDO. Martelar o `write` deixava a thread
// leitora do socket sem conseguir o mutex do registry para enfileirar (o mutex
// do Windows não é justo), e a resposta inteira só aparecia no encerramento.
let ultimo = 0;
while (!fim && Date.now() - t0 < 15000) {
  const agora = Date.now();
  if (agora !== ultimo) { ultimo = agora; s.write(""); v = v + 1; }
}
console.log("bytes:", corpo.length, "| voltas:", v);
console.log(corpo.substring(0, 200));
