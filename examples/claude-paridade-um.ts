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



// Um alvo por PROCESSO. O corpus corria-os em sequência e só o primeiro
// respondia — uma sessão TLS de cada vez, portanto, e é isso que este ficheiro
// contorna sem esconder (o relatório diz que foi preciso).
const HOST = process.env.ALVO_HOST as string;
const PATH = process.env.ALVO_PATH as string;
const NOME = process.env.ALVO_NOME as string;

const html = buscar(HOST, PATH);
writeFileSync("paridade/" + NOME + ".html", html);
let css = "";
let i = 0;
for (;;) {
  const abre = html.indexOf("<link", i);
  if (abre < 0) { break; }
  const fecha = html.indexOf(">", abre);
  if (fecha < 0) { break; }
  const tag = html.substring(abre, fecha);
  i = fecha + 1;
  if (tag.indexOf("stylesheet") < 0) { continue; }
  const k = tag.indexOf("href=");
  if (k < 0) { continue; }
  const aspas = tag.charAt(k + 5);
  const f = tag.indexOf(aspas, k + 6);
  const url = tag.substring(k + 6, f);
  let alvo = url.split("&amp;").join("&");
  if (alvo.indexOf("//") === 0) { alvo = "https:" + alvo; }
  else if (alvo.indexOf("/") === 0) { alvo = "https://" + HOST + alvo; }
  if (alvo.indexOf("https://") !== 0) { continue; }
  const resto = alvo.substring(8);
  const barra = resto.indexOf("/");
  if (barra < 0) { continue; }
  css = css + String.fromCharCode(10) + buscar(resto.substring(0, barra), resto.substring(barra));
}
writeFileSync("paridade/" + NOME + ".css", css);
console.log(NOME + ": html=" + html.length + " css=" + css.length);
console.log("primeiros 200: " + html.substring(0, 200));
