// REFTESTS do Web Platform Tests contra o NOSSO motor — a régua que os
// browsers usam: um `test.html` e a sua referência (`<link rel="match">`)
// têm de renderizar IGUAL. Os dois lados são rasterizados pelo `claude-raster`
// (o rasterizador headless da régua de pintura) e comparados pixel a pixel;
// não precisa de Chrome nem de Edge — é auto-consistência, exactamente como
// o `wptrunner` avalia um reftest.
//
//   cargo build --release -p rts-dom --example claude-raster
//   bun scripts/wpt_reftests.mjs <pasta-do-wpt>/css/css-flexbox [--tol 8] [--max N] [--out dir]
//                                 [--filtro regex] [--esperado N] [--pares match|sufixo] [--sem-png]
//
// O que este número NÃO é: a régua de Blink. Um reftest que passa aqui diz "o
// motor é coerente consigo próprio nestes dois documentos"; um que falha diz
// onde a coerência parte — e é ali que se olha. Limites ditos: o texto é
// medido pelo `ApproxMeasurer` (não há Ahem), por isso um teste cuja
// referência troca texto por caixas pode falhar por fonte e não por layout;
// `rel="mismatch"` fica de fora; testes com `<script>` são corridos SEM JS
// (o rasterizador não tem motor), e é dito na saída quantos são.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, unlinkSync, writeFileSync } from "node:fs";
import { basename, dirname, relative, resolve, join } from "node:path";
import { inflateSync } from "node:zlib";

const args = process.argv.slice(2);
const pasta = args.find((a) => !a.startsWith("--"));
if (!pasta) { console.error("uso: bun scripts/wpt_reftests.mjs <pasta> [--tol 8] [--max N] [--out dir]"); process.exit(2); }
const opt = (n, d) => { const i = args.indexOf("--" + n); return i >= 0 ? args[i + 1] : d; };
const TOL = Number(opt("tol", "8"));
const MAX = Number(opt("max", "0"));
// `--filtro` é para ITERAR num lote, nunca para produzir o número: a saída
// diz-o na primeira linha, e um relatório filtrado não é comparável com o
// `.github/wpt_report.json` do main (denominador diferente — a armadilha que
// o honesty floor chama "verify the input"). Sem filtro, nada muda.
const FILTRO = opt("filtro", "") ? new RegExp(opt("filtro", ""), "i") : null;
// COMO se descobre o par teste/referencia. `match` e a convencao do WPT
// (`<link rel="match" href>`); `sufixo` e a do Blink (`X.html` e um
// reftest se existir `X-expected.html` ao lado). E uma flag e nao um
// script novo porque o resto — rasterizar os dois lados e comparar
// pixel a pixel — e identico, e duas copias divergiriam na tolerancia,
// no timeout e no formato do relatorio, que sao exactamente as tres
// coisas que tornam dois numeros comparaveis.
const PARES = opt("pares", "match");
if (!["match", "sufixo"].includes(PARES)) { console.error(`--pares ${PARES}: use "match" (WPT) ou "sufixo" (Blink)`); process.exit(2); }
const OUT = resolve(opt("out", join(process.env.TEMP ?? ".", "wpt-reftests")));
const RASTER = ["target/release/examples/claude-raster.exe", "target/release/examples/claude-raster"].find(existsSync);
if (!RASTER) { console.error("construa o rasterizador: cargo build --release -p rts-dom --example claude-raster"); process.exit(2); }
// A pasta de saida e LIMPA no arranque. Sem isto ela acumula os PNG de todas as
// corridas anteriores — e como o nome de um teste e estavel, uma falha antiga
// que ja foi corrigida fica la a parecer actual. Uma medicao nova comeca vazia.
if (existsSync(OUT)) rmSync(OUT, { recursive: true, force: true });
mkdirSync(OUT, { recursive: true });
// `--sem-png` nao guarda imagem nenhuma, nem das falhas. Para uma varredura
// larga (o `css` inteiro sao 24 104 reftests) em que so interessa o numero;
// para investigar uma falha, corre-se so essa pasta sem a flag.
const SEM_PNG = args.includes("--sem-png");

// --- PNG mínimo (o mesmo de css_pintura_comparar.mjs: inflate + os 5 filtros)
function decodePng(buf) {
  let p = 8; const chunks = []; let w = 0, h = 0;
  while (p < buf.length) {
    const len = buf.readUInt32BE(p); const type = buf.toString("ascii", p + 4, p + 8);
    const data = buf.subarray(p + 8, p + 8 + len);
    if (type === "IHDR") { w = data.readUInt32BE(0); h = data.readUInt32BE(4); }
    if (type === "IDAT") chunks.push(data);
    p += 12 + len;
  }
  const raw = inflateSync(Buffer.concat(chunks));
  const stride = w * 4; const out = Buffer.alloc(w * h * 4);
  for (let y = 0; y < h; y++) {
    const f = raw[y * (stride + 1)]; const src = y * (stride + 1) + 1; const dst = y * stride;
    for (let i = 0; i < stride; i++) {
      const a = i >= 4 ? out[dst + i - 4] : 0; const b = y > 0 ? out[dst - stride + i] : 0; const c = y > 0 && i >= 4 ? out[dst - stride + i - 4] : 0;
      let pred = 0;
      if (f === 1) pred = a; else if (f === 2) pred = b; else if (f === 3) pred = (a + b) >> 1;
      else if (f === 4) { const pp = a + b - c; const pa = Math.abs(pp - a), pb = Math.abs(pp - b), pc = Math.abs(pp - c); pred = pa <= pb && pa <= pc ? a : pb <= pc ? b : c; }
      out[dst + i] = (raw[src + i] + pred) & 0xff;
    }
  }
  return { w, h, px: out };
}
function diff(a, b) {
  if (a.w !== b.w || a.h !== b.h) return { pct: 100, n: a.w * a.h };
  let dif = 0;
  for (let i = 0; i < a.px.length; i += 4) {
    if (Math.abs(a.px[i] - b.px[i]) > TOL || Math.abs(a.px[i + 1] - b.px[i + 1]) > TOL || Math.abs(a.px[i + 2] - b.px[i + 2]) > TOL) dif++;
  }
  return { pct: (dif / (a.w * a.h)) * 100, n: dif };
}

// --- os testes: `<link rel="match" href="...">`; `mismatch` fica de fora
// RECURSIVO. Era `readdirSync(pasta)` e só via a raiz — `css/css-flexbox` tem
// 533 reftests e o número dizia 489, porque 44 estão em subpastas. Um corpus
// silenciosamente menor do que o nome diz é a armadilha que o honesty floor
// chama "verify the input, not just the output", e ela estava aqui.
// `support/`, `reference/` e as referências apontadas por um teste não são
// testes: um ficheiro só entra se ELE tiver `rel=match`.
function htmlRecursivo(dir) {
  const out = [];
  for (const e of readdirSync(dir, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : 1))) {
    const p = join(dir, e.name);
    if (e.isDirectory()) out.push(...htmlRecursivo(p));
    else if (e.name.endsWith(".html") || e.name.endsWith(".xht")) out.push(p);
  }
  return out;
}
const html = htmlRecursivo(pasta);
const testes = [];
for (const f of html) {
  // Um `-expected.html` e a REFERENCIA de outro teste, nunca um teste.
  if (/-expected\.(html|xht)$/.test(f)) continue;
  let ref = null;
  if (PARES === "sufixo") {
    // O `existsSync` vem ANTES de ler o ficheiro: neste modo o par decide-se
    // pelo NOME, e ler cada html so para descobrir que nao tem par custava a
    // varredura inteira dos web_tests do Blink (dezenas de milhares de
    // ficheiros, 4 046 com par).
    const cand = f.replace(/\.(html|xht)$/, "-expected.$1");
    if (!existsSync(cand)) continue;
    ref = cand;
  }
  const src = readFileSync(f, "utf8");
  if (PARES !== "sufixo") {
    const m = src.match(/<link[^>]*rel=["']?match["']?[^>]*href=["']([^"']+)["']/i) ?? src.match(/<link[^>]*href=["']([^"']+)["'][^>]*rel=["']?match["']?/i);
    if (!m) continue;
    ref = resolve(dirname(f), m[1]);
    if (!existsSync(ref)) continue;
  }
  testes.push({ teste: f, ref, script: /<script/i.test(src) });
}
const filtrados = FILTRO ? testes.filter((t) => FILTRO.test(relative(pasta, t.teste).split("\\").join("/"))) : testes;
// GUARDA de corpus. Uma medicao contra um checkout encolhido nao falha: sai
// um numero mais pequeno, com ar de numero. Aconteceu — `scripts/wpt_reftests.md`
// documenta um `git sparse-checkout set css/css-flexbox ...` como passo de
// instalacao, alguem o correu com a arvore ja alargada, e `css/CSS2` passou de
// 6241 reftests para 102 a meio de uma varredura. Por isso `--esperado N`
// RECUSA correr quando o corpus nao tem o tamanho que quem mede julga estar a
// medir. Sem a flag, nada muda: e uma afirmacao opcional, nao um valor fixo
// que envelhece dentro do script.
const ESPERADO = Number(opt("esperado", "0"));
if (ESPERADO > 0 && testes.length !== ESPERADO) {
  console.error(`corpus com ${testes.length} reftests, esperava ${ESPERADO} — o checkout mudou de tamanho.`);
  console.error(`  para o WPT: cd <wpt> && git sparse-checkout set css resources fonts`);
  process.exit(3);
}
const lista = MAX > 0 ? filtrados.slice(0, MAX) : filtrados;
if (FILTRO) console.log(`--filtro ${FILTRO.source}: ${lista.length} de ${testes.length} — número PARCIAL, não comparável com o relatório do main`);
console.log(`${pasta}: ${html.length} html, ${testes.length} reftests (pares por ${PARES === "sufixo" ? "-expected.html" : "rel=match"}), ${lista.filter((t) => t.script).length} com <script>`);

function rasterizar(htmlPath, png) {
  try { execFileSync(RASTER, [htmlPath, png], { stdio: ["ignore", "ignore", "pipe"], timeout: 20000 }); return true; }
  catch { return false; }
}
// `nao_rasterizaram` guarda os NOMES: um teste que ENCRAVA (timeout do raster)
// não entra em `piores`, e uma comparação por nome entre dois relatórios
// contava-o como GANHO — foi assim que um `flex-aspect-ratio-resize-001` a
// encravar apareceu como teste fechado. Um erro é o pior resultado, não um
// resultado ausente.
// `resultados` guarda TODOS os testes, nao so as falhas: a arvore de
// percentagens por subpasta precisa de saber quantos PASSAM em cada ramo,
// e `piores` (so falhas) daria o numerador sem o denominador. E a mesma
// razao pela qual `nao_rasterizaram` existe a parte — os tres conjuntos
// juntos sao a medicao inteira; qualquer um sozinho e uma vista dela.
let passam = 0, falham = 0, erros = 0; const piores = []; const nao_rasterizaram = []; const resultados = [];
const paraRepetir = [];
// Os PNG de um teste que PASSA sao apagados: sao dois ficheiros identicos que
// ninguem vai abrir, e sao a maioria. Guardados, uma varredura do `css` inteiro
// (24 104 reftests) deixa ~50 mil imagens — 103 GB numa pasta so, medido em
// 2026-09-05 depois de encher o disco desta maquina. Os das FALHAS ficam, que e
// para o que servem: olhar para o que divergiu.
// Cada PNG traz um `.mask.json` ao lado (o raster escreve-o para dizer o que
// mascarou); apagar so o PNG deixava metade dos ficheiros para tras.
function apaga(...paths) {
  for (const f of paths) { try { unlinkSync(f); } catch {} try { unlinkSync(f + ".mask.json"); } catch {} }
}
function julga(t, nome, a, b) {
  const d = diff(decodePng(readFileSync(a)), decodePng(readFileSync(b)));
  if (d.n === 0) {
    passam++; resultados.push({ nome, estado: "passa" });
    apaga(a, b);
  } else {
    falham++;
    piores.push({ nome, pct: d.pct, n: d.n, script: t.script });
    resultados.push({ nome, estado: "falha", pct: d.pct });
    if (SEM_PNG) apaga(a, b);
  }
}
for (const t of lista) {
  // O nome é RELATIVO à pasta, não o `basename`: com a varredura recursiva
  // dois testes de subpastas diferentes podem partilhar o basename, e o nome
  // é a chave da comparação "que reftests perdi" entre dois relatórios —
  // duas linhas com a mesma chave tornariam essa comparação ambígua sem
  // falhar em lado nenhum.
  const nome = relative(pasta, t.teste).split("\\").join("/").replace(/\.(html|xht)$/, "");
  // O ficheiro PNG achata o nome: a chave leva "/" desde que a varredura
  // passou a ser recursiva, e `join` com ele criaria subpastas que nao
  // existem — o raster falharia a escrever e o teste contaria como erro.
  const plano = nome.split("/").join("__");
  const a = join(OUT, plano + ".teste.png"), b = join(OUT, plano + ".ref.png");
  if (!rasterizar(t.teste, a) || !rasterizar(t.ref, b)) { paraRepetir.push({ t, nome, a, b }); continue; }
  julga(t, nome, a, b);
}

// Segunda passagem, EM SERIE e ja sem o resto da varredura a competir. O
// timeout do raster mede tempo de RELOGIO, portanto uma maquina ocupada empurra
// um teste lento para la dele e ele conta como erro — e um erro nao e um
// resultado ausente, e o pior resultado, por isso mentia para baixo. Numa
// medicao feita ao lado de outras tres, 310 dos 870 "nao rasterizaram" e o
// total saiu 380 em vez de 586; os mesmos ficheiros passavam sozinhos. A
// repeticao distingue as duas coisas: carga passa, encravamento real repete.
if (paraRepetir.length > 0) {
  console.log(`\n${paraRepetir.length} nao rasterizaram a primeira vez — a repetir em serie`);
  for (const { t, nome, a, b } of paraRepetir) {
    if (!rasterizar(t.teste, a) || !rasterizar(t.ref, b)) {
      erros++; nao_rasterizaram.push(nome); resultados.push({ nome, estado: "erro" });
    } else julga(t, nome, a, b);
  }
}
piores.sort((x, y) => y.pct - x.pct);
const total = passam + falham + erros;
console.log(`\nWPT reftests — ${passam}/${total} passam (${((passam / Math.max(total, 1)) * 100).toFixed(1)}%), ${falham} falham, ${erros} não rasterizaram; tolerância ${TOL}/255 por canal`);
console.log(`\nos 15 piores:`);
for (const p of piores.slice(0, 15)) console.log(`  ${p.pct.toFixed(2).padStart(6)}%  ${p.n.toString().padStart(7)} px  ${p.nome}${p.script ? "  (tem <script>)" : ""}`);
if (nao_rasterizaram.length > 0) console.log(`NÃO RASTERIZARAM (encravou ou morreu): ${nao_rasterizaram.join(", ")}`);
writeFileSync(join(OUT, "relatorio.json"), JSON.stringify({ pasta, total, passam, falham, erros, nao_rasterizaram, tol: TOL, piores, resultados }, null, 2));
