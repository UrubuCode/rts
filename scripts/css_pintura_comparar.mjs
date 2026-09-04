// O COMPARADOR da régua de pintura: lê o PNG do nosso lado
// (`crates/rts-dom/examples/claude-raster.rs`) e o do Blink
// (`css_fixtures_screenshot_edge.mjs`), e responde por fixture a % de pixels
// diferentes acima de uma tolerância por canal — IGNORANDO as regiões de
// texto do nosso lado (o `.mask.json` que o rasterizador grava ao lado do
// PNG), porque nenhum dos dois lados desta régua tem fonte real: o
// `ApproxMeasurer` não pinta glifo nenhum, e comparar essa área contra o
// Blink mediria o medidor aproximado, não o motor de pintura.
//
//   bun scripts/css_pintura_comparar.mjs claude-cor-e-fundo
//   bun scripts/css_pintura_comparar.mjs                       # todas as que têm os dois PNG
//
// Decodificar PNG: em vez de trazer uma dependência, usamos `node:zlib`
// (disponível no bun) para o `inflate` do IDAT e escrevemos ~40 linhas de
// "unfilter" (os 5 filtros do PNG — None/Sub/Up/Average/Paeth) à mão. Foi
// preferido a gravar `.ppm`/`.rgba` cru dos dois lados porque o
// `Page.captureScreenshot` do CDP só sabe responder PNG (ou JPEG, com
// perdas) — gravar cru do lado Blink exigiria decodificar de qualquer forma,
// então decodificar os dois é o caminho mais curto, não o mais longo.
import { readFileSync, existsSync, readdirSync, writeFileSync } from "node:fs";
import { inflateSync } from "node:zlib";
import { resolve } from "node:path";

const RAIZ = "tests/css";
const PASTA = resolve(RAIZ, "pintura");
const TOLERANCIA = Number(process.env.TOLERANCIA ?? 8); // por canal, 0-255

function decodePng(buf) {
  if (buf.readUInt32BE(0) !== 0x89504e47) throw new Error("assinatura PNG inválida");
  let pos = 8, w = 0, h = 0, bitDepth = 0, colorType = 0;
  const idat = [];
  while (pos < buf.length) {
    const len = buf.readUInt32BE(pos);
    const kind = buf.toString("ascii", pos + 4, pos + 8);
    const data = buf.subarray(pos + 8, pos + 8 + len);
    if (kind === "IHDR") {
      w = data.readUInt32BE(0); h = data.readUInt32BE(4);
      bitDepth = data[8]; colorType = data[9];
    } else if (kind === "IDAT") {
      idat.push(data);
    } else if (kind === "IEND") break;
    pos += 12 + len;
  }
  if (bitDepth !== 8 || (colorType !== 6 && colorType !== 2)) {
    throw new Error(`PNG não suportado: bitDepth=${bitDepth} colorType=${colorType} (só 8-bit RGB/RGBA)`);
  }
  const bpp = colorType === 6 ? 4 : 3;
  const raw = inflateSync(Buffer.concat(idat));
  const stride = w * bpp;
  const out = Buffer.alloc(w * h * 4, 255); // RGBA — canal alpha 255 se a origem era RGB (colorType 2)
  let rpos = 0;
  const prevRow = Buffer.alloc(stride);
  for (let y = 0; y < h; y++) {
    const filter = raw[rpos]; rpos += 1;
    const row = raw.subarray(rpos, rpos + stride); rpos += stride;
    const cur = Buffer.alloc(stride);
    for (let x = 0; x < stride; x++) {
      const a = x >= bpp ? cur[x - bpp] : 0;
      const b = prevRow[x];
      const cc = x >= bpp ? prevRow[x - bpp] : 0;
      let v = row[x];
      switch (filter) {
        case 0: break;
        case 1: v = (v + a) & 0xff; break;
        case 2: v = (v + b) & 0xff; break;
        case 3: v = (v + ((a + b) >> 1)) & 0xff; break;
        case 4: {
          const p = a + b - cc;
          const pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - cc);
          const pr = pa <= pb && pa <= pc ? a : pb <= pc ? b : cc;
          v = (v + pr) & 0xff;
          break;
        }
        default: throw new Error(`filtro PNG desconhecido: ${filter}`);
      }
      cur[x] = v;
    }
    cur.copy(prevRow);
    for (let px = 0; px < w; px++) {
      const si = px * bpp, di = (y * w + px) * 4;
      out[di] = cur[si]; out[di + 1] = cur[si + 1]; out[di + 2] = cur[si + 2];
      out[di + 3] = bpp === 4 ? cur[si + 3] : 255;
    }
  }
  return { w, h, rgba: out };
}

/// Encode PNG mínimo (mesmo esquema do `claude-raster.rs`, sem compressão) —
/// só para o `.diff.png`, que este script também produz.
function encodePngStored(w, h, rgba) {
  const chunks = [];
  const crcTable = (() => {
    const t = new Uint32Array(256);
    for (let n = 0; n < 256; n++) { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; t[n] = c >>> 0; }
    return t;
  })();
  const crc32 = (buf) => { let c = 0xffffffff; for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8); return (c ^ 0xffffffff) >>> 0; };
  const chunk = (kind, data) => {
    const body = Buffer.concat([Buffer.from(kind, "ascii"), data]);
    const len = Buffer.alloc(4); len.writeUInt32BE(data.length);
    const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(body));
    chunks.push(len, body, crc);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8; ihdr[9] = 6; ihdr[10] = 0; ihdr[11] = 0; ihdr[12] = 0;
  chunk("IHDR", ihdr);
  const raw = Buffer.alloc((w * 4 + 1) * h);
  for (let y = 0; y < h; y++) {
    raw[y * (w * 4 + 1)] = 0;
    rgba.copy(raw, y * (w * 4 + 1) + 1, y * w * 4, (y + 1) * w * 4);
  }
  const adler32 = (data) => {
    let a = 1, b = 0;
    for (const byte of data) { a = (a + byte) % 65521; b = (b + a) % 65521; }
    return ((b << 16) | a) >>> 0;
  };
  const zparts = [Buffer.from([0x78, 0x01])];
  for (let i = 0; i < raw.length; i += 65535) {
    const c = raw.subarray(i, Math.min(i + 65535, raw.length));
    const last = i + 65535 >= raw.length ? 1 : 0;
    const hdr = Buffer.alloc(5);
    hdr[0] = last;
    hdr.writeUInt16LE(c.length, 1);
    hdr.writeUInt16LE((~c.length) & 0xffff, 3);
    zparts.push(hdr, c);
  }
  const adlerBuf = Buffer.alloc(4); adlerBuf.writeUInt32BE(adler32(raw));
  zparts.push(adlerBuf);
  chunk("IDAT", Buffer.concat(zparts));
  chunk("IEND", Buffer.alloc(0));
  return Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), ...chunks]);
}

function loadMask(pathPng) {
  const p = `${pathPng}.mask.json`;
  if (!existsSync(p)) return [];
  return JSON.parse(readFileSync(p, "utf8"));
}

function inMask(x, y, mask) {
  for (const [mx, my, mw, mh] of mask) {
    if (x >= mx && x < mx + mw && y >= my && y < my + mh) return true;
  }
  return false;
}

function compararUma(nome) {
  const rtsPath = resolve(PASTA, `${nome}.rts.png`);
  const blinkPath = resolve(PASTA, `${nome}.blink.png`);
  if (!existsSync(rtsPath) || !existsSync(blinkPath)) return null;
  const rts = decodePng(readFileSync(rtsPath));
  const blink = decodePng(readFileSync(blinkPath));
  if (rts.w !== blink.w || rts.h !== blink.h) {
    return { nome, erro: `dimensões diferentes: rts ${rts.w}x${rts.h}, blink ${blink.w}x${blink.h}` };
  }
  const mask = loadMask(rtsPath);
  const { w, h } = rts;
  const diff = Buffer.alloc(w * h * 4, 0);
  let diferentes = 0, ignorados = 0, comparados = 0;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      if (inMask(x, y, mask)) {
        ignorados++;
        diff[i] = 40; diff[i + 1] = 40; diff[i + 2] = 40; diff[i + 3] = 255; // cinza = ignorado
        continue;
      }
      comparados++;
      const dr = Math.abs(rts.rgba[i] - blink.rgba[i]);
      const dg = Math.abs(rts.rgba[i + 1] - blink.rgba[i + 1]);
      const db = Math.abs(rts.rgba[i + 2] - blink.rgba[i + 2]);
      if (dr > TOLERANCIA || dg > TOLERANCIA || db > TOLERANCIA) {
        diferentes++;
        diff[i] = 255; diff[i + 1] = 0; diff[i + 2] = 0; diff[i + 3] = 255;
      } else {
        diff[i] = rts.rgba[i]; diff[i + 1] = rts.rgba[i + 1]; diff[i + 2] = rts.rgba[i + 2]; diff[i + 3] = 255;
      }
    }
  }
  const png = encodePngStored(w, h, diff);
  const diffPath = resolve(PASTA, `${nome}.diff.png`);
  writeFileSync(diffPath, png);
  return {
    nome,
    pct_diferente: comparados ? +((diferentes / comparados) * 100).toFixed(2) : 0,
    pct_area_ignorada: +((ignorados / (w * h)) * 100).toFixed(2),
    diferentes, comparados, ignorados, total: w * h,
  };
}

const pedidos = process.argv.slice(2);
// Aceita `claude-x` e `claude-x.html` (o nome como está em tests/css/). Um
// nome sem `.rts.png` é ERRO dito e exit 2 — a primeira versão respondia
// silêncio e exit 0, que é um instrumento a parecer um resultado.
const nomes = (pedidos.length
  ? pedidos
  : readdirSync(PASTA)
      .filter((n) => n.endsWith(".rts.png"))
      .map((n) => n.replace(/\.rts\.png$/, ""))
).map((n) => n.replace(/\.html$/, ""));
let emFalta = 0;
for (const n of nomes) {
  if (!existsSync(resolve(PASTA, `${n}.rts.png`))) { console.log(`${n}: ERRO sem ${n}.rts.png em ${PASTA} (corra o claude-raster)`); emFalta++; }
  if (!existsSync(resolve(PASTA, `${n}.blink.png`))) { console.log(`${n}: ERRO sem ${n}.blink.png em ${PASTA} (corra o css_fixtures_screenshot_edge)`); emFalta++; }
}

const resultados = nomes.map(compararUma).filter(Boolean);
for (const r of resultados) {
  if (r.erro) console.log(`${r.nome}: ERRO ${r.erro}`);
  else console.log(`${r.nome}: ${r.pct_diferente}% diferente (área ignorada — texto e imagens: ${r.pct_area_ignorada}%)`);
}
if (emFalta) process.exit(2);
