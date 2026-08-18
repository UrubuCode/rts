// Carrega uma PÁGINA DA WEB como um browser carrega e a mostra na janela, tudo
// pelo motor novo.
//
//   cargo run --release -p rts-host --features ui --example ui_fixture -- \
//       examples/claude-web-viewer.ts [url]
//
// O caminho: `node:tls` abre a conexão, a requisição HTTP é escrita à mão sobre
// ela, os `<link rel=stylesheet>` são BUSCADOS pelo mesmo transporte e embutidos
// como `<style>` (o `rts-dom` faz a cascata sobre o documento, não sobre a
// rede), e `egui.html` parseia para a árvore retida e pinta a display list.
// Nenhum browser participa.
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

/// O fim-de-linha do HTTP. Numa constante porque escrevê-lo à mão em cada
/// `indexOf` é onde ele se perde numa edição — foi o que aconteceu, e o que a
/// página mostrou foi um `5203` (um tamanho de chunk) em vez do conteúdo.
const SEP = String.fromCharCode(13) + String.fromCharCode(10);

/// Uma resposta HTTP sobre TLS: só o corpo, sem os cabeçalhos.
///
/// A espera é ativa e bombeia UMA VEZ POR MILISSEGUNDO: este exemplo roda na
/// thread da janela e não tem outro trabalho antes de a página chegar, e
/// martelar o `write` deixa a thread leitora do socket sem o mutex do registry
/// — a resposta inteira só chegava no encerramento do processo.
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
  let bruto = "";
  let fim = false;
  const s: any = connect({ host: host, port: 443, servername: host } as any);
  s.on("secureConnect", () => {
    s.write("GET " + caminho + " HTTP/1.1" + SEP + "Host: " + host + SEP + CHROME +
            "Connection: close" + SEP + SEP);
  });
  s.on("data", (p: any) => { bruto = bruto + p.toString("latin1"); });
  s.on("end", () => { fim = true; });
  s.on("close", () => { fim = true; });
  // O `'error'` do nosso `node:tls` chega SEM objeto (o `emit` passa
  // `undefined`), então ler `.message` direto rebentava o programa a meio do
  // carregamento — defensivo até o TLS construir um `Error` de verdade.
  s.on("error", (e: any) => { console.log("  erro:", e ? e.message : "(sem detalhe)"); fim = true; });

  const t0 = Date.now();
  let ultimo = 0;
  while (!fim && Date.now() - t0 < 20000) {
    const agora = Date.now();
    if (agora !== ultimo) { ultimo = agora; s.write(""); }
  }
  const corte = bruto.indexOf(SEP + SEP);
  const cabecalho = corte < 0 ? "" : bruto.substring(0, corte);
  const corpo = corte < 0 ? bruto : bruto.substring(corte + 4);
  return cabecalho.toLowerCase().indexOf("transfer-encoding: chunked") >= 0
    ? desmontarChunked(corpo)
    : corpo;
}

/// Junta os pedaços de um corpo `Transfer-Encoding: chunked`.
///
/// Não é cosmético: os tamanhos vêm em hexa ENTRE os pedaços, e sem os remover
/// o que aparecia na página era um `5203` solto seguido de conteúdo cortado —
/// o parser tolera o lixo entre elementos, o leitor não.
function desmontarChunked(corpo: string): string {
  let saida = "";
  let i = 0;
  for (;;) {
    const fimLinha = corpo.indexOf(SEP, i);
    if (fimLinha < 0) { break; }
    // a linha pode trazer extensões depois de `;` — o tamanho é o que vem antes
    const linha = corpo.substring(i, fimLinha);
    const pontoVirgula = linha.indexOf(";");
    const hexa = (pontoVirgula < 0 ? linha : linha.substring(0, pontoVirgula)).trim();
    const tamanho = parseInt(hexa, 16);
    if (!(tamanho > 0)) { break; }   // 0 termina, NaN é corpo malformado
    saida = saida + corpo.substring(fimLinha + 2, fimLinha + 2 + tamanho);
    i = fimLinha + 2 + tamanho + 2;  // salta o CRLF que fecha o pedaço
  }
  return texto(saida);
}

/// Os `href` dos `<link rel=stylesheet>` do documento, na ordem em que aparecem
/// — que é a ordem da cascata.
function folhas(fonte: string): string[] {
  const saida: string[] = [];
  let i = 0;
  for (;;) {
    const abre = fonte.indexOf("<link", i);
    if (abre < 0) { break; }
    const fecha = fonte.indexOf(">", abre);
    if (fecha < 0) { break; }
    const tag = fonte.substring(abre, fecha);
    i = fecha + 1;
    if (tag.indexOf("stylesheet") < 0) { continue; }
    const chave = tag.indexOf("href=");
    if (chave < 0) { continue; }
    const aspas = tag.charAt(chave + 5);
    const fim = tag.indexOf(aspas, chave + 6);
    if (fim < 0) { continue; }
    saida.push(tag.substring(chave + 6, fim));
  }
  return saida;
}

/// `https://host/caminho` partido nos dois. Só absoluto: uma página real cita a
/// CDN dela por URL inteiro, e resolver relativo pede uma base que este exemplo
/// não precisa carregar.
function partir(url: string): string[] {
  if (url.indexOf("https://") !== 0) { return []; }
  const resto = url.substring(8);
  const barra = resto.indexOf("/");
  return barra < 0 ? [resto, "/"] : [resto.substring(0, barra), resto.substring(barra)];
}

const ALVO = "https://web.whatsapp.com/";
const partes = partir(ALVO);
console.log("carregando", ALVO);
let fonte = buscar(partes[0], partes[1]);
console.log("  documento:", fonte.length, "bytes");

// Os sub-recursos, como um browser: cada folha buscada e EMBUTIDA. Embutir e
// não referenciar porque a cascata do `rts-dom` corre sobre o documento — a
// rede é problema de quem carrega, e é aqui.
let css = "";
const links = folhas(fonte);
for (let k = 0; k < links.length; k = k + 1) {
  const p = partir(links[k]);
  if (p.length === 0) { continue; }
  const texto = buscar(p[0], p[1]);
  console.log("  folha:", p[0], texto.length, "bytes");
  css = css + "\n" + texto;
}
if (css.length > 0) {
  fonte = "<style>" + css + "</style>" + fonte;
  console.log("  css embutido:", css.length, "bytes de", links.length, "folhas");
}

const win = openWindow("rts-dom — " + ALVO, 1100, 780, 0);
if (win <= 0) {
  console.log("não abriu a janela");
} else {
  let frames = 0;
  while (isOpen(win)) {
    pump(win);
    beginFrame(win);
    html(win, fonte);
    drawText(win, "rts-dom · " + ALVO + " · " + fonte.length + " bytes · frame " + frames, 0);
    endFrame(win);
    frames = frames + 1;
  }
  close(win);
  console.log("fechou depois de", frames, "frames");
}
