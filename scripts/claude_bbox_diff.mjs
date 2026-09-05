// Onde é que dois PNG diferem — a caixa envolvente da divergência, e uma
// amostra das cores dos dois lados.
//
//   bun scripts/claude_bbox_diff.mjs a.png b.png [--tol 8]
//
// O corredor de reftests diz QUANTOS pixels diferem; para corrigir é preciso
// saber ONDE e DE QUE COR. Sem isto, quem investiga um reftest falhado sem
// poder abrir a imagem (um agente sem display, por exemplo) só pode
// especular — e especular sobre geometria é como se estragam correcções que
// já estavam certas.
import { readFileSync } from "node:fs";
import { inflateSync } from "node:zlib";

const args = process.argv.slice(2);
const [fa, fb] = args.filter((a) => !a.startsWith("--"));
const i = args.indexOf("--tol");
const TOL = i >= 0 ? Number(args[i + 1]) : 8;
if (!fa || !fb) { console.error("uso: bun scripts/claude_bbox_diff.mjs a.png b.png [--tol 8]"); process.exit(2); }

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
    for (let k = 0; k < stride; k++) {
      const a = k >= 4 ? out[dst + k - 4] : 0; const b = y > 0 ? out[dst - stride + k] : 0;
      const c = y > 0 && k >= 4 ? out[dst - stride + k - 4] : 0;
      let pred = 0;
      if (f === 1) pred = a; else if (f === 2) pred = b; else if (f === 3) pred = (a + b) >> 1;
      else if (f === 4) { const pp = a + b - c; const pa = Math.abs(pp - a), pb = Math.abs(pp - b), pc = Math.abs(pp - c); pred = pa <= pb && pa <= pc ? a : pb <= pc ? b : c; }
      out[dst + k] = (raw[src + k] + pred) & 0xff;
    }
  }
  return { w, h, px: out };
}

const A = decodePng(readFileSync(fa)), B = decodePng(readFileSync(fb));
if (A.w !== B.w || A.h !== B.h) { console.log(`tamanhos diferentes: ${A.w}x${A.h} vs ${B.w}x${B.h}`); process.exit(0); }

let x0 = Infinity, y0 = Infinity, x1 = -1, y1 = -1, n = 0;
const cores = new Map();
for (let y = 0; y < A.h; y++) {
  for (let x = 0; x < A.w; x++) {
    const k = (y * A.w + x) * 4;
    if (Math.abs(A.px[k] - B.px[k]) <= TOL && Math.abs(A.px[k + 1] - B.px[k + 1]) <= TOL && Math.abs(A.px[k + 2] - B.px[k + 2]) <= TOL) continue;
    n++;
    if (x < x0) x0 = x; if (x > x1) x1 = x;
    if (y < y0) y0 = y; if (y > y1) y1 = y;
    const par = `rgb(${A.px[k]},${A.px[k + 1]},${A.px[k + 2]}) -> rgb(${B.px[k]},${B.px[k + 1]},${B.px[k + 2]})`;
    cores.set(par, (cores.get(par) ?? 0) + 1);
  }
}
if (n === 0) { console.log("idênticos (dentro da tolerância)"); process.exit(0); }
console.log(`${n} px diferem, de ${A.w}x${A.h}`);
console.log(`caixa envolvente: x ${x0}..${x1} (largura ${x1 - x0 + 1}), y ${y0}..${y1} (altura ${y1 - y0 + 1})`);
console.log(`cores (as 6 mais frequentes, A -> B):`);
for (const [par, k] of [...cores].sort((a, b) => b[1] - a[1]).slice(0, 6)) console.log(`  ${String(k).padStart(7)} px   ${par}`);
