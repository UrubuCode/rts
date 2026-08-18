// Baixa uma página por socket CRU: a requisição HTTP escrita na mão.
import { Socket } from "node:net";

const s: any = new Socket();
let corpo = "";
let fim = false;
s.on("connect", () => {
  s.write("GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n");
});
s.on("data", (pedaco: any) => { corpo = corpo + pedaco.toString("utf8"); });
s.on("end", () => { fim = true; });
s.on("close", () => { fim = true; });
s.on("error", (e: any) => { console.log("erro:", e.message); fim = true; });
s.connect(80, "example.com");

// O laço é bloqueante, então nada cede ao loop de eventos: `write("")` é o que
// bombeia o registry do `node:net` (o hook de escrita chama `pump`).
let v = 0;
while (!fim && v < 300000) { s.write(""); v = v + 1; }
console.log("bytes:", corpo.length, "| voltas:", v);
console.log(corpo.substring(0, 120));
