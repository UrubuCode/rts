// Grava `wa.html` e `wa.css` — a página real e a folha externa dela, buscadas
// pela nossa pilha TLS. Existe separado do relatório para que a análise não
// pague uma ida à rede a cada execução.
import { connect } from "node:tls";

const SEP = String.fromCharCode(13) + String.fromCharCode(10);
import { writeFileSync } from "node:fs";

/// Os cabeçalhos de um Chrome, e é assim que um servidor nos identifica: pelo
/// que ESCREVEMOS no pedido, nada mais. O `User-Agent` sozinho não chega — com
/// ele e sem os `sec-ch-ua`/`Accept-Language` o `web.whatsapp.com` responde
/// "Sorry, something went wrong"; com o conjunto responde a página real (593 KB
/// em vez dos 48 KB da landing de browser não suportado).
const CHROME =
  "User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36" + SEP +
  "Accept: text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8" + SEP +
  "Accept-Language: pt-BR,pt;q=0.9,en-US;q=0.8,en;q=0.7" + SEP +
  "sec-ch-ua: \"Chromium\";v=\"140\", \"Google Chrome\";v=\"140\", \"Not?A_Brand\";v=\"24\"" + SEP +
  "sec-ch-ua-mobile: ?0" + SEP +
  "sec-ch-ua-platform: \"Windows\"" + SEP +
  "sec-fetch-site: none" + SEP + "sec-fetch-mode: navigate" + SEP +
  "sec-fetch-user: ?1" + SEP + "sec-fetch-dest: document" + SEP +
  "upgrade-insecure-requests: 1" + SEP;

/// De volta a texto: os bytes chegam como `latin1` (um byte, um caractere) para
/// que os tamanhos de chunk — que são em BYTES — batam com os índices da
/// string. Decodificar como UTF-8 à chegada desalinhava tudo no primeiro
/// acento, e o corpo saía cortado nos primeiros 39 KB de 590.
function texto(bytes: string): string {
  return Buffer.from(bytes, "latin1").toString("utf8");
}

function buscar(host: string, caminho: string): string {
  let bruto = ""; let fim = false;
  const s: any = connect({ host: host, port: 443, servername: host } as any);
  s.on("secureConnect", () => {
    s.write("GET " + caminho + " HTTP/1.1" + SEP + "Host: " + host + SEP + CHROME +
            "Connection: close" + SEP + SEP);
  });
  s.on("data", (p: any) => { bruto = bruto + p.toString("latin1"); });
  s.on("end", () => { fim = true; });
  s.on("close", () => { fim = true; });
  s.on("error", (e: any) => { console.log("erro:", e ? e.message : "(sem detalhe)"); fim = true; });
  const t0 = Date.now(); let u = 0;
  while (!fim && Date.now() - t0 < 20000) { const a = Date.now(); if (a !== u) { u = a; s.write(""); } }
  const corte = bruto.indexOf("\r\n\r\n");
  const cab = corte < 0 ? "" : bruto.substring(0, corte);
  const corpo = corte < 0 ? bruto : bruto.substring(corte + 4);
  if (cab.toLowerCase().indexOf("transfer-encoding: chunked") < 0) { return texto(corpo); }
  let saida = ""; let i = 0;
  for (;;) {
    const fl = corpo.indexOf("\r\n", i);
    if (fl < 0) { break; }
    const linha = corpo.substring(i, fl);
    const pv = linha.indexOf(";");
    const tam = parseInt((pv < 0 ? linha : linha.substring(0, pv)).trim(), 16);
    if (!(tam > 0)) { break; }
    saida = saida + corpo.substring(fl + 2, fl + 2 + tam);
    i = fl + 2 + tam + 2;
  }
  return texto(saida);
}



// Diagnóstico: um alvo por PROCESSO (o `corpus` corre-os em sequência e os
// segundos falhavam), e imprime o CABEÇALHO em vez de o descartar — um 301 ou
// um `content-encoding` que não desmontamos é uma página que não se mede, e
// isso tem de aparecer em vez de virar "0 bytes".
const HOST = process.env.ALVO_HOST as string;
const PATH = process.env.ALVO_PATH as string;
const NOME = process.env.ALVO_NOME as string;

console.log("HOST=[" + HOST + "] PATH=[" + PATH + "]");
let bruto2 = ""; let fim2 = false;
const s2: any = connect({ host: HOST, port: 443, servername: HOST } as any);
s2.on("secureConnect", () => {
  s2.write("GET " + PATH + " HTTP/1.1" + SEP + "Host: " + HOST + SEP + CHROME + "Connection: close" + SEP + SEP);
});
s2.on("data", (p: any) => { bruto2 = bruto2 + p.toString("latin1"); });
s2.on("end", () => { fim2 = true; });
s2.on("close", () => { fim2 = true; });
s2.on("error", (e: any) => { console.log("ERRO-TLS:", e ? e.message : "(sem detalhe)"); fim2 = true; });
const t02 = Date.now(); let u2 = 0;
while (!fim2 && Date.now() - t02 < 25000) { const a = Date.now(); if (a !== u2) { u2 = a; s2.write(""); } }
const c2 = bruto2.indexOf(String.fromCharCode(13) + String.fromCharCode(10) + String.fromCharCode(13) + String.fromCharCode(10));
console.log(NOME + " | bytes=" + bruto2.length);
console.log(c2 < 0 ? "(sem cabecalho)" : bruto2.substring(0, c2 > 700 ? 700 : c2));
