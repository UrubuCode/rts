// Régua ad-hoc para o lote flex-abspos-static-bfc: rasteriza os 7 reftests do
// WPT deste lote (teste + referência) e compara pixel a pixel, exactamente
// como scripts/wpt_reftests.mjs (mesmo decodificador PNG, mesma tolerância).
// Existe à parte porque só interessam ESTES 7 nomes, não a pasta inteira.
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { basename, join } from "node:path";
import { inflateSync } from "node:zlib";

const WPT = "C:\\Users\\nexga\\AppData\\Local\\Temp\\wpt\\css\\css-flexbox";
// DEBUG, nunca release — o coordenador constrói a release; um agente só
// verifica com `cargo run`/`cargo build` sem `--release` (feedback do lote).
const RASTER = "target/debug/examples/claude-raster.exe";
const OUT = "target/lote-reftests";
const TOL = 8;

const nomes = [
  "align-items-006",
  "flexbox-min-width-auto-005",
  "flexbox-definite-sizes-003",
  "flexbox-definite-sizes-004",
  "flex-abspos-inset-nested-001",
  "flex-abspos-inset-nested-002",
  "flex-item-position-relative-001",
];

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
  if (a.w !== b.w || a.h !== b.h) return { pct: 100, n: a.w * a.h, dimMismatch: true, aw: a.w, ah: a.h, bw: b.w, bh: b.h };
  let dif = 0;
  for (let i = 0; i < a.px.length; i += 4) {
    if (Math.abs(a.px[i] - b.px[i]) > TOL || Math.abs(a.px[i + 1] - b.px[i + 1]) > TOL || Math.abs(a.px[i + 2] - b.px[i + 2]) > TOL) dif++;
  }
  return { pct: (dif / (a.w * a.h)) * 100, n: dif };
}
function rasterizar(htmlPath, png) {
  try { execFileSync(RASTER, [htmlPath, png], { stdio: ["ignore", "ignore", "pipe"], timeout: 20000 }); return true; }
  catch (e) { return e.stdout?.toString() ?? e.message; }
}

import { mkdirSync } from "node:fs";
mkdirSync(OUT, { recursive: true });

for (const nome of nomes) {
  const teste = join(WPT, nome + ".html");
  const src = readFileSync(teste, "utf8");
  const m = src.match(/<link[^>]*rel=["']?match["']?[^>]*href=["']([^"']+)["']/i) ?? src.match(/<link[^>]*href=["']([^"']+)["'][^>]*rel=["']?match["']?/i);
  if (!m) { console.log(`${nome}: SEM <link rel=match>`); continue; }
  const ref = join(WPT, m[1]);
  if (!existsSync(ref)) { console.log(`${nome}: referência ${ref} não existe`); continue; }
  const a = join(OUT, nome + ".teste.png"), b = join(OUT, nome + ".ref.png");
  const ra = rasterizar(teste, a);
  const rb = rasterizar(ref, b);
  if (ra !== true || rb !== true) {
    console.log(`${nome}: NÃO RASTERIZOU (teste=${ra === true ? "ok" : ra}, ref=${rb === true ? "ok" : rb})`);
    continue;
  }
  const d = diff(decodePng(readFileSync(a)), decodePng(readFileSync(b)));
  if (d.dimMismatch) {
    console.log(`${nome}: DIMENSÕES DIFERENTES teste=${d.aw}x${d.ah} ref=${d.bw}x${d.bh}`);
  } else if (d.n === 0) {
    console.log(`${nome}: PASSA (0 px diferentes)`);
  } else {
    console.log(`${nome}: FALHA ${d.pct.toFixed(2)}% (${d.n} px) — ref: ${ref}`);
  }
}
