// REFTESTS do Web Platform Tests contra o NOSSO motor — a régua que os
// browsers usam: um `test.html` e a sua referência (`<link rel="match">`)
// têm de renderizar IGUAL. Os dois lados são rasterizados pelo `claude-raster`
// (o rasterizador headless da régua de pintura) e comparados pixel a pixel;
// não precisa de Chrome nem de Edge — é auto-consistência, exactamente como
// o `wptrunner` avalia um reftest.
//
//   cargo build --release -p rts-dom --example claude-raster
//   bun scripts/wpt_reftests.mjs <pasta-do-wpt>/css/css-flexbox [--tol 8] [--max N] [--out dir] [--filtro regex]
//
// O que este número NÃO é: a régua de Blink. Um reftest que passa aqui diz "o
// motor é coerente consigo próprio nestes dois documentos"; um que falha diz
// onde a coerência parte — e é ali que se olha. Limites ditos: o texto é
// medido pelo `ApproxMeasurer` (não há Ahem), por isso um teste cuja
// referência troca texto por caixas pode falhar por fonte e não por layout;
// `rel="mismatch"` fica de fora; testes com `<script>` são corridos SEM JS
// (o rasterizador não tem motor), e é dito na saída quantos são.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { basename, dirname, resolve, join } from "node:path";
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
const OUT = resolve(opt("out", join(process.env.TEMP ?? ".", "wpt-reftests")));
const RASTER = ["target/release/examples/claude-raster.exe", "target/release/examples/claude-raster"].find(existsSync);
if (!RASTER) { console.error("construa o rasterizador: cargo build --release -p rts-dom --example claude-raster"); process.exit(2); }
mkdirSync(OUT, { recursive: true });

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
const html = readdirSync(pasta).filter((f) => f.endsWith(".html") || f.endsWith(".xht")).sort();
const testes = [];
for (const f of html) {
  const src = readFileSync(join(pasta, f), "utf8");
  const m = src.match(/<link[^>]*rel=["']?match["']?[^>]*href=["']([^"']+)["']/i) ?? src.match(/<link[^>]*href=["']([^"']+)["'][^>]*rel=["']?match["']?/i);
  if (!m) continue;
  const ref = resolve(pasta, m[1]);
  if (!existsSync(ref)) continue;
  testes.push({ teste: join(pasta, f), ref, script: /<script/i.test(src) });
}
const filtrados = FILTRO ? testes.filter((t) => FILTRO.test(basename(t.teste))) : testes;
const lista = MAX > 0 ? filtrados.slice(0, MAX) : filtrados;
if (FILTRO) console.log(`--filtro ${FILTRO.source}: ${lista.length} de ${testes.length} — número PARCIAL, não comparável com o relatório do main`);
console.log(`${pasta}: ${html.length} html, ${testes.length} reftests (rel=match com referência existente), ${lista.filter((t) => t.script).length} com <script>`);

function rasterizar(htmlPath, png) {
  try { execFileSync(RASTER, [htmlPath, png], { stdio: ["ignore", "ignore", "pipe"], timeout: 20000 }); return true; }
  catch { return false; }
}
// `nao_rasterizaram` guarda os NOMES: um teste que ENCRAVA (timeout do raster)
// não entra em `piores`, e uma comparação por nome entre dois relatórios
// contava-o como GANHO — foi assim que um `flex-aspect-ratio-resize-001` a
// encravar apareceu como teste fechado. Um erro é o pior resultado, não um
// resultado ausente.
let passam = 0, falham = 0, erros = 0; const piores = []; const nao_rasterizaram = [];
for (const t of lista) {
  const nome = basename(t.teste).replace(/\.(html|xht)$/, "");
  const a = join(OUT, nome + ".teste.png"), b = join(OUT, nome + ".ref.png");
  if (!rasterizar(t.teste, a) || !rasterizar(t.ref, b)) { erros++; nao_rasterizaram.push(nome); continue; }
  const d = diff(decodePng(readFileSync(a)), decodePng(readFileSync(b)));
  if (d.n === 0) passam++; else { falham++; piores.push({ nome, pct: d.pct, n: d.n, script: t.script }); }
}
piores.sort((x, y) => y.pct - x.pct);
const total = passam + falham + erros;
console.log(`\nWPT reftests — ${passam}/${total} passam (${((passam / Math.max(total, 1)) * 100).toFixed(1)}%), ${falham} falham, ${erros} não rasterizaram; tolerância ${TOL}/255 por canal`);
console.log(`\nos 15 piores:`);
for (const p of piores.slice(0, 15)) console.log(`  ${p.pct.toFixed(2).padStart(6)}%  ${p.n.toString().padStart(7)} px  ${p.nome}${p.script ? "  (tem <script>)" : ""}`);
if (nao_rasterizaram.length > 0) console.log(`NÃO RASTERIZARAM (encravou ou morreu): ${nao_rasterizaram.join(", ")}`);
writeFileSync(join(OUT, "relatorio.json"), JSON.stringify({ pasta, total, passam, falham, erros, nao_rasterizaram, tol: TOL, piores }, null, 2));
