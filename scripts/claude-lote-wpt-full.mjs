// Verificação PRÓPRIA do lote flex-abspos-static-bfc: corre TODOS os reftests
// de `css/css-flexbox` (não só os 7 do brief) contra o binário DEBUG — nunca
// `--release` — para ver se a correção fechou mais do que os 7 nomeados. Não
// substitui a medição do coordenador no fecho da vaga (que usa release);
// serve só para o relatório deste lote dizer "e também X, Y, Z" com números.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { basename, resolve, join } from "node:path";
import { inflateSync } from "node:zlib";

const WPT = "C:\\Users\\nexga\\AppData\\Local\\Temp\\wpt\\css\\css-flexbox";
const RASTER = "target/debug/examples/claude-raster.exe";
const OUT = "target/lote-wpt-full";
const TOL = 8;

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
function rasterizar(htmlPath, png) {
  try { execFileSync(RASTER, [htmlPath, png], { stdio: ["ignore", "ignore", "pipe"], timeout: 20000 }); return true; }
  catch { return false; }
}

mkdirSync(OUT, { recursive: true });
const html = readdirSync(WPT).filter((f) => f.endsWith(".html") || f.endsWith(".xht")).sort();
const testes = [];
for (const f of html) {
  const src = readFileSync(join(WPT, f), "utf8");
  const m = src.match(/<link[^>]*rel=["']?match["']?[^>]*href=["']([^"']+)["']/i) ?? src.match(/<link[^>]*href=["']([^"']+)["'][^>]*rel=["']?match["']?/i);
  if (!m) continue;
  const ref = resolve(WPT, m[1]);
  if (!existsSync(ref)) continue;
  testes.push({ teste: join(WPT, f), ref });
}
console.log(`${WPT}: ${html.length} html, ${testes.length} reftests`);
let passam = 0, falham = 0, erros = 0;
const passaram = [];
for (const t of testes) {
  const nome = basename(t.teste).replace(/\.(html|xht)$/, "");
  const a = join(OUT, nome + ".teste.png"), b = join(OUT, nome + ".ref.png");
  if (!rasterizar(t.teste, a) || !rasterizar(t.ref, b)) { erros++; continue; }
  const d = diff(decodePng(readFileSync(a)), decodePng(readFileSync(b)));
  if (d.n === 0) { passam++; passaram.push(nome); } else falham++;
}
const total = passam + falham + erros;
console.log(`\nWPT reftests (debug, claude-lote-wpt-full) — ${passam}/${total} passam, ${falham} falham, ${erros} não rasterizaram`);
writeFileSync(join(OUT, "passaram.json"), JSON.stringify(passaram, null, 2));
