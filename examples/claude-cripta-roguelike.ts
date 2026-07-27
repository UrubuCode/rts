// CRIPTA — rogue-FPS estilo DOOM com MAPA INFINITO por seed (rts:serde + wgpu).
//
//   Um labirinto de pedra INFINITO gerado deterministicamente da seed do run
//   (maze binary-tree por célula — sempre conexo), horda contínua de demônios,
//   upgrades a cada meta de abates, minimapa. Morreu → permadeath, mas as
//   ALMAS viram META-PROGRESSÃO comprável — e o MetaSave (classe) é PICKLADO
//   com rts:serde num .rtsp que sobrevive entre execuções.
//
//   MOUSE — mira (captura FPS)     CLIQUE ESQ — atira
//   W/A/S/D — anda                 ESPAÇO — dash
//   ESC — solta/recaptura o mouse  Q — abandonar o run
//
//   Rodar: target/release/rts.exe run examples/claude-cripta-roguelike.ts
import { egui, input, buffer, time, io } from "rts";
import { serialize, deserialize } from "rts:serde";
import { writeFileSync, readFileSync, existsSync } from "node:fs";

const win: i64 = egui.openWindow("CRIPTA — rogue-FPS infinito (pickle meta-save)", 1280, 760, 0);

// ── META-SAVE PICKLADO ──────────────────────────────────────────────────────
class MetaSave {
  almas: number;
  totalMortes: number;
  melhorNivel: number;
  melhorPontos: number;
  perkDano: number;
  perkVida: number;
  perkVel: number;
  constructor() {
    this.almas = 0;
    this.totalMortes = 0;
    this.melhorNivel = 0;
    this.melhorPontos = 0;
    this.perkDano = 0;
    this.perkVida = 0;
    this.perkVel = 0;
  }
}
const META_PATH = "claude-cripta-meta.rtsp";
let meta = new MetaSave();
if (existsSync(META_PATH)) {
  const m: any = deserialize(readFileSync(META_PATH) as any);
  if (m instanceof MetaSave) {
    meta = m;
    // evolução de schema do pickle: campo novo/renomeado chega undefined —
    // normaliza pra 0 (o save antigo continua válido)
    if (!(meta.almas >= 0)) meta.almas = 0;
    if (!(meta.totalMortes >= 0)) meta.totalMortes = 0;
    if (!(meta.melhorNivel >= 0)) meta.melhorNivel = 0;
    if (!(meta.melhorPontos >= 0)) meta.melhorPontos = 0;
    if (!(meta.perkDano >= 0)) meta.perkDano = 0;
    if (!(meta.perkVida >= 0)) meta.perkVida = 0;
    if (!(meta.perkVel >= 0)) meta.perkVel = 0;
    io.print("[cripta] meta-save: " + meta.almas + " almas, melhor nivel " + meta.melhorNivel);
  }
}
function salvarMeta(): void {
  writeFileSync(META_PATH, serialize(meta) as any);
}

// ── CUBO ────────────────────────────────────────────────────────────────────
function cubeMesh(): i64 {
  const vb: i64 = buffer.alloc(24 * 32);
  const ib: i64 = buffer.alloc(36 * 4);
  let vi = 0;
  for (let f = 0; f < 6; f++) {
    let nx: f64 = 0.0;
    let ny: f64 = 0.0;
    let nz: f64 = 0.0;
    if (f === 0) ny = 1.0;
    if (f === 1) ny = -1.0;
    if (f === 2) nz = 1.0;
    if (f === 3) nz = -1.0;
    if (f === 4) nx = 1.0;
    if (f === 5) nx = -1.0;
    for (let c = 0; c < 4; c++) {
      let x: f64 = -0.5;
      let y: f64 = -0.5;
      let z: f64 = -0.5;
      if (f === 0) { y = 0.5; if (c === 1 || c === 2) x = 0.5; if (c === 2 || c === 3) z = 0.5; }
      if (f === 1) { y = -0.5; if (c === 1 || c === 2) x = 0.5; if (c === 2 || c === 3) z = 0.5; }
      if (f === 2) { z = 0.5; if (c === 1 || c === 2) x = 0.5; if (c === 2 || c === 3) y = 0.5; }
      if (f === 3) { z = -0.5; if (c === 1 || c === 2) x = 0.5; if (c === 2 || c === 3) y = 0.5; }
      if (f === 4) { x = 0.5; if (c === 1 || c === 2) z = 0.5; if (c === 2 || c === 3) y = 0.5; }
      if (f === 5) { x = -0.5; if (c === 1 || c === 2) z = 0.5; if (c === 2 || c === 3) y = 0.5; }
      const base: i64 = vi * 32;
      buffer.write_f32(vb, base, x);
      buffer.write_f32(vb, base + 4, y);
      buffer.write_f32(vb, base + 8, z);
      buffer.write_f32(vb, base + 12, nx);
      buffer.write_f32(vb, base + 16, ny);
      buffer.write_f32(vb, base + 20, nz);
      buffer.write_f32(vb, base + 24, 0.0);
      buffer.write_f32(vb, base + 28, 0.0);
      vi = vi + 1;
    }
    const v: i64 = f * 4;
    const o: i64 = f * 24;
    buffer.write_i32(ib, o, v);
    buffer.write_i32(ib, o + 4, v + 1);
    buffer.write_i32(ib, o + 8, v + 2);
    buffer.write_i32(ib, o + 12, v);
    buffer.write_i32(ib, o + 16, v + 2);
    buffer.write_i32(ib, o + 20, v + 3);
  }
  const id: i64 = egui.meshUpload(win, buffer.ptr(vb), 24, buffer.ptr(ib), 36);
  buffer.free(vb);
  buffer.free(ib);
  return id;
}
const cube: i64 = cubeMesh();
egui.setClearColor(win, 0.015, 0.008, 0.012);

// ── MAPA INFINITO POR SEED ──────────────────────────────────────────────────
// O mundo é uma grade infinita de células CEL×CEL. Cada célula é dona das
// paredes NORTE (z+) e LESTE (x+); um hash determinístico (seed, cx, cz)
// decide as passagens: binary-tree maze (norte OU leste sempre aberto →
// conexo por construção) + aberturas extras pra virar salão (loops).
const CEL: f64 = 14.0;
const ALT: f64 = 4.2;
let seedRun = 0;

function hash2(cx: number, cz: number, salt: number): number {
  let h = (cx * 374761393 + cz * 668265263 + seedRun * 974634269 + salt * 144665461) | 0;
  h = (h ^ (h >> 13)) | 0;
  h = (h * 1274126177) | 0;
  h = (h ^ (h >> 16)) | 0;
  return h & 0x7fffffff;
}

// paredes ATIVAS (células num raio de 2 da célula do jogador)
const muroX: f64[] = [];
const muroZ: f64[] = [];
const muroW: f64[] = [];
const muroD: f64[] = [];
// pilares/tochas ativos (decoração determinística por célula)
const pilX: f64[] = [];
const pilZ: f64[] = [];
const tocX: f64[] = [];
const tocZ: f64[] = [];
let celAtX = 99999;
let celAtZ = 99999;

function celulaDe(v: f64): number {
  return Math.floor(v / CEL);
}

// muro norte/leste da célula com PORTA (vão de 5) em posição hasheada
function empurraParede(cx: number, cz: number, lado: number, aberto: number): void {
  const bx: f64 = cx * CEL;
  const bz: f64 = cz * CEL;
  if (lado === 0) {
    // NORTE: segmento em z = bz + CEL
    const zw: f64 = bz + CEL;
    if (aberto === 0) {
      muroX.push(bx + CEL / 2.0);
      muroZ.push(zw);
      muroW.push(CEL + 0.8);
      muroD.push(0.8);
    } else {
      const off: f64 = 2.5 + (hash2(cx, cz, 7) % 100) / 100.0 * (CEL - 10.0);
      muroX.push(bx + off / 2.0);
      muroZ.push(zw);
      muroW.push(off + 0.8);
      muroD.push(0.8);
      const resto: f64 = CEL - off - 5.0;
      muroX.push(bx + off + 5.0 + resto / 2.0);
      muroZ.push(zw);
      muroW.push(resto + 0.8);
      muroD.push(0.8);
    }
  } else {
    // LESTE: segmento em x = bx + CEL
    const xw: f64 = bx + CEL;
    if (aberto === 0) {
      muroX.push(xw);
      muroZ.push(bz + CEL / 2.0);
      muroW.push(0.8);
      muroD.push(CEL + 0.8);
    } else {
      const off: f64 = 2.5 + (hash2(cx, cz, 11) % 100) / 100.0 * (CEL - 10.0);
      muroX.push(xw);
      muroZ.push(bz + off / 2.0);
      muroW.push(0.8);
      muroD.push(off + 0.8);
      const resto: f64 = CEL - off - 5.0;
      muroX.push(xw);
      muroZ.push(bz + off + 5.0 + resto / 2.0);
      muroW.push(0.8);
      muroD.push(resto + 0.8);
    }
  }
}

function gerarAoRedor(pcx: number, pcz: number): void {
  while (muroX.length > 0) { muroX.pop(); }
  while (muroZ.length > 0) { muroZ.pop(); }
  while (muroW.length > 0) { muroW.pop(); }
  while (muroD.length > 0) { muroD.pop(); }
  while (pilX.length > 0) { pilX.pop(); }
  while (pilZ.length > 0) { pilZ.pop(); }
  while (tocX.length > 0) { tocX.pop(); }
  while (tocZ.length > 0) { tocZ.pop(); }
  for (let cx = pcx - 2; cx <= pcx + 2; cx++) {
    for (let cz = pcz - 2; cz <= pcz + 2; cz++) {
      const h = hash2(cx, cz, 0);
      // binary-tree: norte OU leste aberto; extra abre o outro tb (25%)
      const abreNorte = h & 1;
      const abreLeste = 1 - abreNorte;
      const extra = (h >> 3) % 4 === 0 ? 1 : 0;
      empurraParede(cx, cz, 0, abreNorte === 1 || extra === 1 ? 1 : 0);
      empurraParede(cx, cz, 1, abreLeste === 1 || extra === 1 ? 1 : 0);
      // decoração da célula
      const bx: f64 = cx * CEL;
      const bz: f64 = cz * CEL;
      const deco = (h >> 5) % 5;
      if (deco === 0) {
        // pilar central (colide)
        pilX.push(bx + CEL / 2.0);
        pilZ.push(bz + CEL / 2.0);
      }
      if (deco === 1) {
        // dois pilares em diagonal
        pilX.push(bx + CEL * 0.3);
        pilZ.push(bz + CEL * 0.3);
        pilX.push(bx + CEL * 0.7);
        pilZ.push(bz + CEL * 0.7);
      }
      if ((h >> 8) % 3 === 0) {
        // tocha perto do canto noroeste da célula
        tocX.push(bx + 1.4 + ((h >> 10) % 20) / 20.0 * (CEL - 2.8));
        tocZ.push(bz + CEL - 1.3);
      }
    }
  }
  celAtX = pcx;
  celAtZ = pcz;
}

function colide(cx0: f64, cz0: f64, raio: f64): f64[] {
  let x = cx0;
  let z = cz0;
  for (let i = 0; i < muroX.length; i++) {
    const minX: f64 = muroX[i] - muroW[i] / 2.0;
    const maxX: f64 = muroX[i] + muroW[i] / 2.0;
    const minZ: f64 = muroZ[i] - muroD[i] / 2.0;
    const maxZ: f64 = muroZ[i] + muroD[i] / 2.0;
    const nx: f64 = Math.max(minX, Math.min(x, maxX));
    const nz: f64 = Math.max(minZ, Math.min(z, maxZ));
    const dx: f64 = x - nx;
    const dz: f64 = z - nz;
    const d2: f64 = dx * dx + dz * dz;
    if (d2 < raio * raio) {
      if (d2 > 0.000001) {
        const d: f64 = Math.sqrt(d2);
        x = nx + (dx / d) * raio;
        z = nz + (dz / d) * raio;
      } else {
        const lx: f64 = Math.min(x - minX, maxX - x);
        const lz: f64 = Math.min(z - minZ, maxZ - z);
        if (lx < lz) {
          x = x - minX < maxX - x ? minX - raio : maxX + raio;
        } else {
          z = z - minZ < maxZ - z ? minZ - raio : maxZ + raio;
        }
      }
    }
  }
  // pilares (1.5x1.5)
  for (let i = 0; i < pilX.length; i++) {
    const dx: f64 = x - pilX[i];
    const dz: f64 = z - pilZ[i];
    const rr: f64 = raio + 0.95;
    const d2: f64 = dx * dx + dz * dz;
    if (d2 < rr * rr && d2 > 0.0001) {
      const d: f64 = Math.sqrt(d2);
      x = pilX[i] + (dx / d) * rr;
      z = pilZ[i] + (dz / d) * rr;
    }
  }
  return [x, z];
}

// ── ENTIDADES ───────────────────────────────────────────────────────────────
class Inimigo {
  x: f64;
  z: f64;
  hp: f64;
  hpMax: f64;
  vel: f64;
  raio: f64;
  tipo: number;
  atkCd: f64;
  fase: f64;
  stuckT: f64;
  lastX: f64;
  lastZ: f64;
  constructor(x: f64, z: f64, tipo: number, escala: f64) {
    this.x = x;
    this.z = z;
    this.tipo = tipo;
    if (tipo === 0) {
      this.hpMax = 22.0 * escala;
      this.vel = 4.4;
      this.raio = 0.45;
    } else {
      this.hpMax = 70.0 * escala;
      this.vel = 2.3;
      this.raio = 0.75;
    }
    this.hp = this.hpMax;
    this.atkCd = 0.0;
    this.fase = Math.random() * 6.28;
    this.stuckT = 0.0;
    this.lastX = x;
    this.lastZ = z;
  }
}

class Part {
  x: f64; y: f64; z: f64; vx: f64; vy: f64; vz: f64; ttl: f64; cor: number;
  constructor(x: f64, y: f64, z: f64, vx: f64, vy: f64, vz: f64, ttl: f64, cor: number) {
    this.x = x; this.y = y; this.z = z; this.vx = vx; this.vy = vy; this.vz = vz;
    this.ttl = ttl; this.cor = cor;
  }
}

const UP_NOMES: string[] = [
  "DANO BRUTAL",
  "GATILHO NERVOSO",
  "TIRO TRIPLO",
  "SANGUE POR SANGUE",
  "PES DE VENTO",
  "CASCA GROSSA",
  "OLHO DA MORTE",
];
const UP_DESCS: string[] = [
  "+35% de dano",
  "+30% de cadencia",
  "+2 projeteis, -20% dano",
  "cura 2 HP por abate",
  "+20% de velocidade",
  "+30 HP maximo, cura 30",
  "15% de chance de critico 3x",
];

let nivel = 1;
let kills = 0;
let killsProxUp = 8;
let pontos = 0;
let profund: f64 = 0.0;
let hp: f64 = 100.0;
let hpMax: f64 = 100.0;
let dano: f64 = 12.0;
let cadencia: f64 = 3.2;
let projeteis = 1;
let vampiro = 0;
let velMove: f64 = 7.0;
let critChance: f64 = 0.0;
let inimigos: Inimigo[] = [];
let parts: Part[] = [];
let upA = 0;
let upB = 1;
let upC = 2;

function aplicarPerks(): void {
  dano = 12.0 * (1.0 + 0.10 * meta.perkDano);
  hpMax = 100.0 + 20.0 * meta.perkVida;
  hp = hpMax;
  velMove = 7.0 * (1.0 + 0.10 * meta.perkVel);
  cadencia = 3.2;
  projeteis = 1;
  vampiro = 0;
  critChance = 0.0;
}

let px: f64 = 7.0;
let pz: f64 = 7.0;

function sortearUpgrades(): void {
  upA = Math.floor(Math.random() * 7);
  upB = Math.floor(Math.random() * 7);
  while (upB === upA) upB = Math.floor(Math.random() * 7);
  upC = Math.floor(Math.random() * 7);
  while (upC === upA || upC === upB) upC = Math.floor(Math.random() * 7);
}

function aplicarUpgrade(id: number): void {
  if (id === 0) dano = dano * 1.35;
  if (id === 1) cadencia = cadencia * 1.3;
  if (id === 2) { projeteis = projeteis + 2; dano = dano * 0.8; }
  if (id === 3) vampiro = vampiro + 2;
  if (id === 4) velMove = velMove * 1.2;
  if (id === 5) { hpMax = hpMax + 30.0; hp = Math.min(hpMax, hp + 30.0); }
  if (id === 6) critChance = critChance + 0.15;
}

const T_INICIO = 0;
const T_JOGO = 1;
const T_UPGRADE = 2;
const T_MORTO = 3;
let tela = T_INICIO;

let yaw: f64 = 0.0;
let pitch: f64 = 0.0;
let shootCd: f64 = 0.0;
let dashCd: f64 = 0.0;
let dashT: f64 = 0.0;
let dashDX: f64 = 0.0;
let dashDZ: f64 = 0.0;
let flashDano: f64 = 0.0;
let flashTiro: f64 = 0.0;
let bobT: f64 = 0.0;
let tglobal: f64 = 0.0;
let mouseCapturado = 0;
let toast = "";
let toastT: f64 = 0.0;

const KEY_W: i64 = 122;
const KEY_A: i64 = 100;
const KEY_S: i64 = 118;
const KEY_D: i64 = 103;
const KEY_SPACE: i64 = 3;
const KEY_ESC: i64 = 2;
const KEY_Q: i64 = 116;
const KEY_R: i64 = 117;

function comecarRun(): void {
  seedRun = 100000 + Math.floor(Math.random() * 899999);
  nivel = 1;
  kills = 0;
  killsProxUp = 8;
  pontos = 0;
  profund = 0.0;
  px = 7.0;
  pz = 7.0;
  yaw = 0.0;
  pitch = 0.0;
  parts = [];
  inimigos = [];
  celAtX = 99999;
  aplicarPerks();
  tela = T_JOGO;
  mouseCapturado = 1;
  egui.mouseLock(win, 1);
}

function morrer(): void {
  meta.totalMortes = meta.totalMortes + 1;
  if (nivel > meta.melhorNivel) meta.melhorNivel = nivel;
  if (pontos > meta.melhorPontos) meta.melhorPontos = pontos;
  salvarMeta();
  tela = T_MORTO;
  mouseCapturado = 0;
  egui.mouseLock(win, 0);
}

// ── UI (cores do canvas 2D = 0xRRGGBBAA, alpha no FIM) ──────────────────────
// botão trabalhado: sombra dupla + moldura + corpo em degradê (3 faixas) +
// vidro no topo + cantoneiras douradas + hover com glow e lift
function botao(x: f64, y0: f64, w: f64, h: f64, tituloB: string, sub: string, clicou: number, quente: number): number {
  const mx: f64 = input.mouseX(win);
  const my: f64 = input.mouseY(win);
  const hover = mx >= x && mx <= x + w && my >= y0 && my <= y0 + h ? 1 : 0;
  const segurando = hover === 1 && input.mouseDown(win, 0) != 0 ? 1 : 0;
  let y = y0;
  if (hover === 1 && segurando === 0) y = y0 - 2;
  if (segurando === 1) y = y0 + 1;
  // sombra dupla
  egui.drawRect(win, x + 5, y + 8, w, h, 0x00000060, 0, 0, 12);
  egui.drawRect(win, x + 2, y + 4, w, h, 0x00000090, 0, 0, 12);
  // moldura externa
  let molCor = 0x4A2A38FF;
  if (quente === 1) molCor = 0x6E3A48FF;
  if (hover === 1) molCor = 0xC89040FF;
  egui.drawRect(win, x - 2, y - 2, w + 4, h + 4, 0x0D060AFF, 2, molCor, 12);
  // corpo em degradê vertical (3 faixas)
  let c1 = 0x352030FF;
  let c2 = 0x281826FF;
  let c3 = 0x1C1018FF;
  if (quente === 1) { c1 = 0x4A2432FF; c2 = 0x381A26FF; c3 = 0x28121AFF; }
  if (hover === 1) { c1 = 0x5A3040FF; c2 = 0x452434FF; c3 = 0x321A26FF; }
  egui.drawRect(win, x, y, w, h, c3, 0, 0, 10);
  egui.drawRect(win, x, y, w, h * 0.62, c2, 0, 0, 10);
  egui.drawRect(win, x, y, w, h * 0.34, c1, 0, 0, 10);
  // vidro no topo
  egui.drawRect(win, x + 4, y + 3, w - 8, h * 0.2, 0xFFFFFF1E, 0, 0, 8);
  // vinco inferior
  egui.drawRect(win, x + 4, y + h - 4, w - 8, 2, 0x00000066, 0, 0, 0);
  // cantoneiras douradas
  let canto = 0x9A7040C0;
  if (hover === 1) canto = 0xFFCC66FF;
  egui.drawRect(win, x + 4, y + 4, 14, 2, canto, 0, 0, 0);
  egui.drawRect(win, x + 4, y + 4, 2, 14, canto, 0, 0, 0);
  egui.drawRect(win, x + w - 18, y + 4, 14, 2, canto, 0, 0, 0);
  egui.drawRect(win, x + w - 6, y + 4, 2, 14, canto, 0, 0, 0);
  egui.drawRect(win, x + 4, y + h - 6, 14, 2, canto, 0, 0, 0);
  egui.drawRect(win, x + 4, y + h - 18, 2, 14, canto, 0, 0, 0);
  egui.drawRect(win, x + w - 18, y + h - 6, 14, 2, canto, 0, 0, 0);
  egui.drawRect(win, x + w - 6, y + h - 18, 2, 14, canto, 0, 0, 0);
  // glow no hover
  if (hover === 1) {
    egui.drawRect(win, x - 6, y - 6, w + 12, h + 12, 0x00000000, 2, 0xE0A04055, 16);
  }
  // texto centrado com sombra
  const tsz: f64 = sub === "" ? 19 : 17;
  const tw: f64 = egui.measureText(win, tituloB, tsz, 1);
  let corT = 0xE8D4D8FF;
  if (hover === 1) corT = 0xFFEFC0FF;
  const ty: f64 = sub === "" ? y + h / 2.0 - tsz * 0.62 : y + h * 0.18;
  egui.drawText(win, x + (w - tw) / 2.0 + 1, ty + 2, tituloB, 0x000000AA, tsz, 1);
  egui.drawText(win, x + (w - tw) / 2.0, ty, tituloB, corT, tsz, 1);
  if (sub !== "") {
    const sw: f64 = egui.measureText(win, sub, 13, 0);
    egui.drawText(win, x + (w - sw) / 2.0, y + h * 0.58, sub, 0xB09AA6FF, 13, 0);
  }
  return hover === 1 && clicou === 1 ? 1 : 0;
}

function tituloSombra(x: f64, y: f64, texto: string, cor: number, tam: f64): void {
  egui.drawText(win, x + 3, y + 5, texto, 0x000000CC, tam, 1);
  egui.drawText(win, x, y, texto, cor, tam, 1);
}

io.print("[cripta] pronto — almas acumuladas: " + meta.almas);

let last: f64 = time.now_ms();
while (egui.isOpen(win)) {
  if (egui.pump(win) != 0) break;
  egui.beginFrame(win);
  const now: f64 = time.now_ms();
  let dt: f64 = (now - last) / 1000.0;
  last = now;
  if (dt > 0.1) dt = 0.1;
  tglobal = tglobal + dt;

  const W: f64 = egui.winWidth(win);
  const H: f64 = egui.winHeight(win);
  const clicou = input.mouseClicked(win, 0);

  if (tela !== T_JOGO) {
    yaw = tglobal * 0.12;
    pitch = -0.14;
  }
  const fx: f64 = Math.sin(yaw);
  const fz: f64 = Math.cos(yaw);
  let andando = 0;

  // regenera o pedaço ativo do labirinto quando muda de célula
  if (tela === T_JOGO || tela === T_UPGRADE) {
    const pcx = celulaDe(px);
    const pcz = celulaDe(pz);
    if (pcx !== celAtX || pcz !== celAtZ) {
      gerarAoRedor(pcx, pcz);
    }
  }

  if (tela === T_JOGO) {
    if (mouseCapturado === 1) {
      yaw = yaw + input.mouseDeltaX(win) * 0.0032;
      pitch = pitch - input.mouseDeltaY(win) * 0.0032;
      pitch = Math.max(-1.2, Math.min(1.2, pitch));
    }
    const rgx: f64 = Math.cos(yaw);
    const rgz: f64 = -Math.sin(yaw);
    let mx2: f64 = 0.0;
    let mz2: f64 = 0.0;
    if (input.key(win, KEY_W, 0) != 0) { mx2 = mx2 + fx; mz2 = mz2 + fz; }
    if (input.key(win, KEY_S, 0) != 0) { mx2 = mx2 - fx; mz2 = mz2 - fz; }
    if (input.key(win, KEY_D, 0) != 0) { mx2 = mx2 + rgx; mz2 = mz2 + rgz; }
    if (input.key(win, KEY_A, 0) != 0) { mx2 = mx2 - rgx; mz2 = mz2 - rgz; }
    const ml: f64 = Math.sqrt(mx2 * mx2 + mz2 * mz2);
    dashCd = Math.max(0.0, dashCd - dt);
    if (input.key(win, KEY_SPACE, 1) != 0 && dashCd <= 0.0 && ml > 0.001) {
      dashT = 0.16;
      dashCd = 1.1;
      dashDX = mx2 / ml;
      dashDZ = mz2 / ml;
    }
    if (dashT > 0.0) {
      dashT = dashT - dt;
      px = px + dashDX * 26.0 * dt;
      pz = pz + dashDZ * 26.0 * dt;
      andando = 1;
    } else if (ml > 0.001) {
      px = px + (mx2 / ml) * velMove * dt;
      pz = pz + (mz2 / ml) * velMove * dt;
      andando = 1;
      bobT = bobT + dt * 9.0;
    }
    const pc = colide(px, pz, 0.45);
    px = pc[0];
    pz = pc[1];
    const dOrigem: f64 = Math.sqrt(px * px + pz * pz);
    if (dOrigem > profund) profund = dOrigem;

    // ── HORDA: mantém a pressão constante ──────────────────────────────────
    const alvoVivos = 4 + nivel * 2;
    if (inimigos.length < alvoVivos) {
      const ang: f64 = Math.random() * 6.28;
      const rd: f64 = 16.0 + Math.random() * 6.0;
      const sx: f64 = px + Math.sin(ang) * rd;
      const sz: f64 = pz + Math.cos(ang) * rd;
      const sc = colide(sx, sz, 0.8);
      let tipo = 0;
      if (nivel >= 2 && Math.random() < 0.3) tipo = 1;
      inimigos.push(new Inimigo(sc[0], sc[1], tipo, 1.0 + (nivel - 1) * 0.22));
    }

    // ── atirar ─────────────────────────────────────────────────────────────
    shootCd = shootCd - dt;
    if (input.mouseDown(win, 0) != 0 && shootCd <= 0.0 && mouseCapturado === 1) {
      shootCd = 1.0 / cadencia;
      flashTiro = 0.05;
      for (let p = 0; p < projeteis; p++) {
        let sy: f64 = yaw;
        if (projeteis > 1) sy = yaw + (p - (projeteis - 1) / 2.0) * 0.07;
        const dxr: f64 = Math.sin(sy) * Math.cos(pitch);
        const dyr: f64 = Math.sin(pitch);
        const dzr: f64 = Math.cos(sy) * Math.cos(pitch);
        let melhor = -1;
        let melhorT: f64 = 999.0;
        for (let i = 0; i < inimigos.length; i++) {
          const e = inimigos[i];
          const ox: f64 = e.x - px;
          const oy: f64 = 0.9 - 1.7;
          const oz: f64 = e.z - pz;
          const tproj: f64 = ox * dxr + oy * dyr + oz * dzr;
          if (tproj < 0.3 || tproj > 60.0) continue;
          const cxq: f64 = ox - dxr * tproj;
          const cyq: f64 = oy - dyr * tproj;
          const czq: f64 = oz - dzr * tproj;
          const d2: f64 = cxq * cxq + cyq * cyq + czq * czq;
          const rr: f64 = e.raio + 0.25;
          if (d2 < rr * rr && tproj < melhorT) {
            melhorT = tproj;
            melhor = i;
          }
        }
        if (melhor >= 0) {
          const e = inimigos[melhor];
          let d: f64 = dano;
          let corHit = 0xFFCC2222; // partículas usam cor 3D (0xAARRGGBB)
          if (Math.random() < critChance) {
            d = d * 3.0;
            corHit = 0xFFFFD040;
          }
          e.hp = e.hp - d;
          for (let b = 0; b < 5; b++) {
            parts.push(new Part(
              e.x, 0.9, e.z,
              (Math.random() - 0.5) * 5.0, 2.0 + Math.random() * 3.0, (Math.random() - 0.5) * 5.0,
              0.5, corHit));
          }
          if (e.hp <= 0.0) {
            kills = kills + 1;
            pontos = pontos + (e.tipo === 1 ? 25 : 10);
            meta.almas = meta.almas + (e.tipo === 1 ? 3 : 1);
            if (vampiro > 0) hp = Math.min(hpMax, hp + vampiro);
            for (let b = 0; b < 14; b++) {
              parts.push(new Part(
                e.x, 1.0, e.z,
                (Math.random() - 0.5) * 8.0, 2.0 + Math.random() * 5.0, (Math.random() - 0.5) * 8.0,
                0.8, 0xFF881111));
            }
            inimigos[melhor] = inimigos[inimigos.length - 1];
            inimigos.pop();
            if (kills >= killsProxUp) {
              sortearUpgrades();
              tela = T_UPGRADE;
              mouseCapturado = 0;
              egui.mouseLock(win, 0);
            }
          }
        }
      }
    }

    // ── IA ─────────────────────────────────────────────────────────────────
    for (let i = 0; i < inimigos.length; i++) {
      const e = inimigos[i];
      const dx2: f64 = px - e.x;
      const dz2: f64 = pz - e.z;
      const d: f64 = Math.sqrt(dx2 * dx2 + dz2 * dz2);
      if (d > e.raio + 0.6) {
        e.x = e.x + (dx2 / d) * e.vel * dt;
        e.z = e.z + (dz2 / d) * e.vel * dt;
        const ec = colide(e.x, e.z, e.raio);
        e.x = ec[0];
        e.z = ec[1];
      }
      const mvx: f64 = e.x - e.lastX;
      const mvz: f64 = e.z - e.lastZ;
      if (d > 10.0 && mvx * mvx + mvz * mvz < 0.0004) {
        e.stuckT = e.stuckT + dt;
        if (e.stuckT > 2.5) {
          const ang: f64 = Math.random() * 6.28;
          const rd: f64 = 9.0 + Math.random() * 4.0;
          const tc = colide(px + Math.sin(ang) * rd, pz + Math.cos(ang) * rd, e.raio);
          e.x = tc[0];
          e.z = tc[1];
          e.stuckT = 0.0;
        }
      } else {
        e.stuckT = 0.0;
      }
      e.lastX = e.x;
      e.lastZ = e.z;
      e.atkCd = e.atkCd - dt;
      if (d < e.raio + 0.9 && e.atkCd <= 0.0 && dashT <= 0.0) {
        e.atkCd = 0.8;
        hp = hp - (e.tipo === 1 ? 18.0 : 8.0);
        flashDano = 0.25;
        if (hp <= 0.0) {
          morrer();
        }
      }
      for (let j = i + 1; j < inimigos.length; j++) {
        const o = inimigos[j];
        const sx2: f64 = o.x - e.x;
        const sz2: f64 = o.z - e.z;
        const sd: f64 = sx2 * sx2 + sz2 * sz2;
        const md: f64 = e.raio + o.raio;
        if (sd > 0.0001 && sd < md * md) {
          const s: f64 = Math.sqrt(sd);
          const push: f64 = (md - s) * 0.5;
          o.x = o.x + (sx2 / s) * push;
          o.z = o.z + (sz2 / s) * push;
          e.x = e.x - (sx2 / s) * push;
          e.z = e.z - (sz2 / s) * push;
        }
      }
    }

    if (input.key(win, KEY_ESC, 1) != 0) {
      if (mouseCapturado === 1) {
        mouseCapturado = 0;
        egui.mouseLock(win, 0);
        toast = "mouse solto — ESC recaptura";
        toastT = 2.0;
      } else {
        mouseCapturado = 1;
        egui.mouseLock(win, 1);
      }
    }
    if (input.key(win, KEY_Q, 1) != 0) {
      morrer();
    }
  }

  // ── partículas ──────────────────────────────────────────────────────────
  let pi = 0;
  while (pi < parts.length) {
    const p = parts[pi];
    p.ttl = p.ttl - dt;
    if (p.ttl <= 0.0) {
      parts[pi] = parts[parts.length - 1];
      parts.pop();
    } else {
      p.vy = p.vy - 12.0 * dt;
      p.x = p.x + p.vx * dt;
      p.y = Math.max(0.03, p.y + p.vy * dt);
      p.z = p.z + p.vz * dt;
      pi = pi + 1;
    }
  }

  // ══ RENDER 3D (cores drawMesh = 0xAARRGGBB) ══════════════════════════════
  const flicker: f64 = 0.30 + Math.sin(tglobal * 13.0) * 0.02 + Math.sin(tglobal * 31.0) * 0.015;
  egui.setLight(win, px, 3.2, pz, flicker);
  egui.setShadow(win, 0.25, -1.0, 0.18, px, 0, pz, 26);

  const bob: f64 = andando === 1 ? Math.sin(bobT) * 0.05 : 0.0;
  const ey: f64 = 1.7 + bob;
  egui.setCamera(win, px, ey, pz, yaw, pitch, 1.25, W / H);

  // chão + teto acompanham o jogador (mundo infinito)
  egui.drawMesh(win, cube, px, -0.5, pz, 0, 0, 76, 1, 76, 0xFF2E2228, 0, 1);
  egui.drawMesh(win, cube, px, ALT + 0.5, pz, 0, 0, 76, 1, 76, 0xFF17090E, 0, 0);

  for (let i = 0; i < muroX.length; i++) {
    const mw: f64 = muroW[i];
    const md: f64 = muroD[i];
    egui.drawMesh(win, cube, muroX[i], ALT / 2.0, muroZ[i], 0, 0, mw, ALT, md, 0xFF463038, 0, 0);
    egui.drawMesh(win, cube, muroX[i], 0.22, muroZ[i], 0, 0, mw + 0.16, 0.44, md + 0.16, 0xFF5C4048, 0, 0);
    egui.drawMesh(win, cube, muroX[i], ALT - 0.14, muroZ[i], 0, 0, mw + 0.12, 0.28, md + 0.12, 0xFF332028, 0, 0);
  }
  // pilares com base e capitel
  for (let i = 0; i < pilX.length; i++) {
    egui.drawMesh(win, cube, pilX[i], ALT / 2.0, pilZ[i], 0, 0, 1.5, ALT, 1.5, 0xFF3E2A32, 0, 0);
    egui.drawMesh(win, cube, pilX[i], 0.3, pilZ[i], 0, 0, 2.0, 0.6, 2.0, 0xFF543A44, 0, 0);
    egui.drawMesh(win, cube, pilX[i], ALT - 0.3, pilZ[i], 0, 0, 1.9, 0.5, 1.9, 0xFF2E1E26, 0, 0);
  }
  // tochas
  for (let i = 0; i < tocX.length; i++) {
    const pulso: f64 = 0.16 + Math.sin(tglobal * 9.0 + i * 1.7) * 0.035;
    egui.drawMesh(win, cube, tocX[i], 2.1, tocZ[i], 0, 0.6, 0.09, 0.7, 0.09, 0xFF3A2A22, 0, 0);
    egui.drawMesh(win, cube, tocX[i], 2.62, tocZ[i], tglobal * 3.0, tglobal * 5.0, pulso, pulso * 1.6, pulso, 0xFFFF8822, 1, 0);
    egui.drawMesh(win, cube, tocX[i], 2.5, tocZ[i], 0, tglobal * 2.0, 0.12, 0.1, 0.12, 0xFFCC4411, 1, 0);
  }

  // demônios
  for (let i = 0; i < inimigos.length; i++) {
    const e = inimigos[i];
    const face: f64 = Math.atan2(px - e.x, pz - e.z);
    const passo: f64 = Math.sin(tglobal * 9.0 + e.fase);
    const bobE: f64 = Math.abs(passo) * 0.05;
    const lx: f64 = Math.cos(face);
    const lz: f64 = -Math.sin(face);
    const ffx: f64 = Math.sin(face);
    const ffz: f64 = Math.cos(face);
    if (e.tipo === 0) {
      egui.drawMesh(win, cube, e.x, 0.55 + bobE, e.z, 0.15, face, 0.66, 0.95, 0.5, 0xFF6E1C1C, 0, 0);
      egui.drawMesh(win, cube, e.x + ffx * 0.12, 1.22 + bobE, e.z + ffz * 0.12, 0.1, face, 0.42, 0.38, 0.42, 0xFF833030, 0, 0);
      egui.drawMesh(win, cube, e.x + lx * 0.14 + ffx * 0.1, 1.5 + bobE, e.z + lz * 0.14 + ffz * 0.1, 0.5, face, 0.07, 0.26, 0.07, 0xFFD8C8A8, 0, 0);
      egui.drawMesh(win, cube, e.x - lx * 0.14 + ffx * 0.1, 1.5 + bobE, e.z - lz * 0.14 + ffz * 0.1, 0.5, face, 0.07, 0.26, 0.07, 0xFFD8C8A8, 0, 0);
      egui.drawMesh(win, cube, e.x + lx * 0.4, 0.7 + passo * 0.12, e.z + lz * 0.4, passo * 0.5, face, 0.14, 0.5, 0.14, 0xFF5E1616, 0, 0);
      egui.drawMesh(win, cube, e.x - lx * 0.4, 0.7 - passo * 0.12, e.z - lz * 0.4, -passo * 0.5, face, 0.14, 0.5, 0.14, 0xFF5E1616, 0, 0);
      egui.drawMesh(win, cube, e.x + lx * 0.11 + ffx * 0.32, 1.26 + bobE, e.z + lz * 0.11 + ffz * 0.32, 0, face, 0.07, 0.07, 0.05, 0xFFFFCC22, 1, 0);
      egui.drawMesh(win, cube, e.x - lx * 0.11 + ffx * 0.32, 1.26 + bobE, e.z - lz * 0.11 + ffz * 0.32, 0, face, 0.07, 0.07, 0.05, 0xFFFFCC22, 1, 0);
    } else {
      egui.drawMesh(win, cube, e.x, 0.95 + bobE, e.z, 0, face, 1.15, 1.7, 0.85, 0xFF3E2450, 0, 0);
      egui.drawMesh(win, cube, e.x + lx * 0.72, 1.6 + bobE, e.z + lz * 0.72, 0.2, face, 0.5, 0.42, 0.5, 0xFF4A2C5E, 0, 0);
      egui.drawMesh(win, cube, e.x - lx * 0.72, 1.6 + bobE, e.z - lz * 0.72, -0.2, face, 0.5, 0.42, 0.5, 0xFF4A2C5E, 0, 0);
      egui.drawMesh(win, cube, e.x + ffx * 0.1, 2.12 + bobE, e.z + ffz * 0.1, 0, face, 0.6, 0.52, 0.6, 0xFF56346A, 0, 0);
      egui.drawMesh(win, cube, e.x + lx * 0.34 + ffx * 0.08, 2.52 + bobE, e.z + lz * 0.34 + ffz * 0.08, 0.7, face, 0.11, 0.4, 0.11, 0xFFE0D0B0, 0, 0);
      egui.drawMesh(win, cube, e.x - lx * 0.34 + ffx * 0.08, 2.52 + bobE, e.z - lz * 0.34 + ffz * 0.08, 0.7, face, 0.11, 0.4, 0.11, 0xFFE0D0B0, 0, 0);
      egui.drawMesh(win, cube, e.x + lx * 0.75, 0.75 + passo * 0.15, e.z + lz * 0.75, passo * 0.4, face, 0.26, 0.6, 0.26, 0xFF34204A, 0, 0);
      egui.drawMesh(win, cube, e.x - lx * 0.75, 0.75 - passo * 0.15, e.z - lz * 0.75, -passo * 0.4, face, 0.26, 0.6, 0.26, 0xFF34204A, 0, 0);
      egui.drawMesh(win, cube, e.x + lx * 0.16 + ffx * 0.34, 2.18 + bobE, e.z + lz * 0.16 + ffz * 0.34, 0, face, 0.1, 0.1, 0.06, 0xFFFF3322, 1, 0);
      egui.drawMesh(win, cube, e.x - lx * 0.16 + ffx * 0.34, 2.18 + bobE, e.z - lz * 0.16 + ffz * 0.34, 0, face, 0.1, 0.1, 0.06, 0xFFFF3322, 1, 0);
    }
    const frac: f64 = e.hp / e.hpMax;
    const bw: f64 = e.tipo === 1 ? 1.4 : 0.9;
    const by: f64 = e.tipo === 1 ? 3.0 : 1.85;
    egui.drawMesh(win, cube, e.x, by + bobE, e.z, 0, face, bw, 0.1, 0.05, 0xFF14080C, 0, 0);
    egui.drawMesh(win, cube, e.x - lx * (bw * (1.0 - frac)) / 2.0, by + bobE, e.z - lz * (bw * (1.0 - frac)) / 2.0, 0, face, bw * frac, 0.12, 0.07, 0xFF33DD44, 1, 0);
  }

  for (let i = 0; i < parts.length; i++) {
    const p = parts[i];
    egui.drawMesh(win, cube, p.x, p.y, p.z, p.ttl * 6.0, p.ttl * 9.0, 0.12, 0.12, 0.12, p.cor, 1, 0);
  }

  // arma viewmodel
  if (tela === T_JOGO) {
    const cpv: f64 = Math.cos(pitch);
    const spv: f64 = Math.sin(pitch);
    const rgx2: f64 = Math.cos(yaw);
    const rgz2: f64 = -Math.sin(yaw);
    const recuo: f64 = flashTiro > 0.0 ? 0.1 : 0.0;
    const wob: f64 = bob * 0.6;
    const gx: f64 = px + fx * cpv * (0.72 - recuo) + rgx2 * 0.3;
    const gy: f64 = ey + spv * 0.72 - 0.26 + wob;
    const gz: f64 = pz + fz * cpv * (0.72 - recuo) + rgz2 * 0.3;
    egui.drawMesh(win, cube, gx, gy, gz, pitch, yaw, 0.13, 0.17, 0.5, 0xFF23232C, 0, 0);
    egui.drawMesh(win, cube, gx + fx * 0.32, gy + spv * 0.32, gz + fz * 0.32, pitch, yaw, 0.07, 0.07, 0.34, 0xFF3C3C48, 0, 0);
    egui.drawMesh(win, cube, gx - fx * 0.12, gy - 0.14, gz - fz * 0.12, pitch + 0.5, yaw, 0.09, 0.22, 0.12, 0xFF2A1A14, 0, 0);
    if (flashTiro > 0.0) {
      flashTiro = flashTiro - dt;
      egui.drawMesh(win, cube, gx + fx * 0.58, gy + spv * 0.58, gz + fz * 0.58, tglobal * 20.0, yaw, 0.24, 0.24, 0.24, 0xFFFFCC44, 1, 0);
    }
  }

  // ══ HUD / TELAS (cores 2D = 0xRRGGBBAA) ══════════════════════════════════
  if (tela === T_JOGO) {
    // crosshair
    egui.drawLine(win, W / 2 - 12, H / 2, W / 2 - 4, H / 2, 2, 0xEEDDCCFF);
    egui.drawLine(win, W / 2 + 4, H / 2, W / 2 + 12, H / 2, 2, 0xEEDDCCFF);
    egui.drawLine(win, W / 2, H / 2 - 12, W / 2, H / 2 - 4, 2, 0xEEDDCCFF);
    egui.drawLine(win, W / 2, H / 2 + 4, W / 2, H / 2 + 12, 2, 0xEEDDCCFF);
    // MINIMAPA (canto sup. direito): paredes ativas + demônios + jogador
    const mmS: f64 = 170;
    const mmX: f64 = W - mmS - 18;
    const mmY: f64 = 18;
    const esc: f64 = mmS / 76.0;
    egui.drawRect(win, mmX - 3, mmY - 3, mmS + 6, mmS + 6, 0x0D060AE6, 2, 0x6A3648FF, 6);
    for (let i = 0; i < muroX.length; i++) {
      const rx: f64 = mmX + (muroX[i] - muroW[i] / 2.0 - (px - 38.0)) * esc;
      const rz: f64 = mmY + mmS - (muroZ[i] + muroD[i] / 2.0 - (pz - 38.0)) * esc;
      egui.drawRect(win, rx, rz, Math.max(2.0, muroW[i] * esc), Math.max(2.0, muroD[i] * esc), 0x8A6070C8, 0, 0, 0);
    }
    for (let i = 0; i < inimigos.length; i++) {
      const rx: f64 = mmX + (inimigos[i].x - (px - 38.0)) * esc;
      const rz: f64 = mmY + mmS - (inimigos[i].z - (pz - 38.0)) * esc;
      if (rx > mmX && rx < mmX + mmS && rz > mmY && rz < mmY + mmS) {
        egui.drawRect(win, rx - 2, rz - 2, 4, 4, 0xFF4433FF, 0, 0, 2);
      }
    }
    egui.drawRect(win, mmX + mmS / 2.0 - 2, mmY + mmS / 2.0 - 2, 5, 5, 0x66DDFFFF, 0, 0, 2);
    // painel inferior
    egui.drawRect(win, 0, H - 64, W, 64, 0x140A10E6, 0, 0, 0);
    egui.drawRect(win, 0, H - 66, W, 2, 0x6A3040FF, 0, 0, 0);
    egui.drawRect(win, 18, H - 50, 280, 30, 0x1A1016FF, 2, 0x6A3040FF, 5);
    const hfrac: f64 = Math.max(0.0, hp / hpMax);
    let hcor = 0x30C24AFF;
    if (hfrac < 0.55) hcor = 0xD0A020FF;
    if (hfrac < 0.28) hcor = 0xC22030FF;
    egui.drawRect(win, 20, H - 48, 276.0 * hfrac, 26, hcor, 0, 0, 4);
    egui.drawText(win, 30, H - 46, "HP " + Math.ceil(hp) + " / " + hpMax, 0xFFF0F2FF, 16, 1);
    egui.drawText(win, 320, H - 47, "NIVEL " + nivel, 0xE0B060FF, 20, 1);
    egui.drawText(win, 435, H - 45, kills + "/" + killsProxUp + " abates p/ bencao", 0xC8A8B0FF, 14, 0);
    egui.drawText(win, 650, H - 45, pontos + " pts", 0xD8C8CCFF, 16, 0);
    egui.drawText(win, 760, H - 45, "prof " + Math.round(profund) + "m", 0x9A8890FF, 14, 0);
    egui.drawText(win, W - 250, H - 45, meta.almas + " almas | seed " + seedRun, 0xB090E0FF, 13, 0);
    if (flashDano > 0.0) {
      flashDano = flashDano - dt;
      egui.drawRect(win, 0, 0, W, H, 0xCC112250, 0, 0, 0);
    }
    if (toastT > 0.0) {
      toastT = toastT - dt;
      const twt: f64 = egui.measureText(win, toast, 15, 0);
      egui.drawText(win, (W - twt) / 2.0, 74, toast, 0xFFD060FF, 15, 0);
    }
  } else if (tela === T_INICIO) {
    egui.drawRect(win, 0, 0, W, H, 0x00000088, 0, 0, 0);
    const pw: f64 = 780;
    const ph0: f64 = 490;
    const px0: f64 = (W - pw) / 2.0;
    const py0: f64 = (H - ph0) / 2.0 - 10;
    egui.drawRect(win, px0 + 6, py0 + 10, pw, ph0, 0x000000A0, 0, 0, 18);
    egui.drawRect(win, px0, py0, pw, ph0, 0x1A0F16F5, 2, 0x6A3648FF, 18);
    egui.drawRect(win, px0 + 4, py0 + 4, pw - 8, 74, 0xFFFFFF14, 0, 0, 14);
    // filete dourado sob o título
    const tw2: f64 = egui.measureText(win, "C R I P T A", 48, 1);
    tituloSombra(px0 + (pw - tw2) / 2.0, py0 + 20, "C R I P T A", 0xE05060FF, 48);
    egui.drawRect(win, px0 + pw / 2.0 - 120, py0 + 86, 240, 2, 0xC89040AA, 0, 0, 0);
    const st = "labirinto infinito por seed — as almas persistem num pickle (rts:serde)";
    const stw: f64 = egui.measureText(win, st, 14, 0);
    egui.drawText(win, px0 + (pw - stw) / 2.0, py0 + 98, st, 0xA890A0FF, 14, 0);
    const stats = meta.almas + " almas   |   " + meta.totalMortes + " mortes   |   melhor: nivel " + meta.melhorNivel + " — " + meta.melhorPontos + " pts";
    const sw2: f64 = egui.measureText(win, stats, 15, 0);
    egui.drawRect(win, px0 + 30, py0 + 126, pw - 60, 34, 0x221420FF, 1, 0x4A2A3AFF, 8);
    egui.drawText(win, px0 + (pw - sw2) / 2.0, py0 + 134, stats, 0xCBB0E0FF, 15, 0);
    const custoD = (meta.perkDano + 1) * 15;
    const custoV = (meta.perkVida + 1) * 15;
    const custoS = (meta.perkVel + 1) * 15;
    const bw2: f64 = (pw - 90) / 3.0;
    if (botao(px0 + 30, py0 + 184, bw2, 82, "FORCA  " + meta.perkDano + "/5", "+10% dano — " + custoD + " almas", clicou, 0) === 1) {
      if (meta.perkDano < 5 && meta.almas >= custoD) {
        meta.almas = meta.almas - custoD;
        meta.perkDano = meta.perkDano + 1;
        salvarMeta();
      }
    }
    if (botao(px0 + 45 + bw2, py0 + 184, bw2, 82, "VIGOR  " + meta.perkVida + "/5", "+20 HP — " + custoV + " almas", clicou, 0) === 1) {
      if (meta.perkVida < 5 && meta.almas >= custoV) {
        meta.almas = meta.almas - custoV;
        meta.perkVida = meta.perkVida + 1;
        salvarMeta();
      }
    }
    if (botao(px0 + 60 + bw2 * 2, py0 + 184, bw2, 82, "PRESSA  " + meta.perkVel + "/5", "+10% vel — " + custoS + " almas", clicou, 0) === 1) {
      if (meta.perkVel < 5 && meta.almas >= custoS) {
        meta.almas = meta.almas - custoS;
        meta.perkVel = meta.perkVel + 1;
        salvarMeta();
      }
    }
    if (botao(px0 + (pw - 340) / 2.0, py0 + 306, 340, 78, "DESCER NA CRIPTA", "", clicou, 1) === 1) {
      comecarRun();
    }
    const ctl = "WASD move  |  MOUSE mira  |  CLIQUE atira  |  ESPACO dash  |  Q abandona";
    const cw2: f64 = egui.measureText(win, ctl, 13, 0);
    egui.drawText(win, px0 + (pw - cw2) / 2.0, py0 + 420, ctl, 0x8A7680FF, 13, 0);
  } else if (tela === T_UPGRADE) {
    egui.drawRect(win, 0, 0, W, H, 0x00000090, 0, 0, 0);
    const tit = "NIVEL " + nivel + " VENCIDO — escolha sua bencao";
    const tw3: f64 = egui.measureText(win, tit, 24, 1);
    tituloSombra((W - tw3) / 2.0, 116, tit, 0xE0C060FF, 24);
    egui.drawRect(win, W / 2.0 - 130, 152, 260, 2, 0xC89040AA, 0, 0, 0);
    const cw3: f64 = 280;
    if (botao(W / 2 - cw3 * 1.5 - 30, 190, cw3, 116, UP_NOMES[upA], UP_DESCS[upA], clicou, 1) === 1) {
      aplicarUpgrade(upA);
      nivel = nivel + 1;
      killsProxUp = kills + 8 + nivel * 4;
      tela = T_JOGO;
      mouseCapturado = 1;
      egui.mouseLock(win, 1);
    }
    if (botao(W / 2 - cw3 / 2.0, 190, cw3, 116, UP_NOMES[upB], UP_DESCS[upB], clicou, 1) === 1) {
      aplicarUpgrade(upB);
      nivel = nivel + 1;
      killsProxUp = kills + 8 + nivel * 4;
      tela = T_JOGO;
      mouseCapturado = 1;
      egui.mouseLock(win, 1);
    }
    if (botao(W / 2 + cw3 / 2.0 + 30, 190, cw3, 116, UP_NOMES[upC], UP_DESCS[upC], clicou, 1) === 1) {
      aplicarUpgrade(upC);
      nivel = nivel + 1;
      killsProxUp = kills + 8 + nivel * 4;
      tela = T_JOGO;
      mouseCapturado = 1;
      egui.mouseLock(win, 1);
    }
    const inf = "hp " + Math.ceil(hp) + "/" + hpMax + "   dano " + Math.round(dano) + "   cadencia " + (Math.round(cadencia * 10.0) / 10.0) + "/s   vel " + (Math.round(velMove * 10.0) / 10.0);
    const iw: f64 = egui.measureText(win, inf, 14, 0);
    egui.drawText(win, (W - iw) / 2.0, 336, inf, 0xC0B0B8FF, 14, 0);
  } else if (tela === T_MORTO) {
    egui.drawRect(win, 0, 0, W, H, 0x1A0006AA, 0, 0, 0);
    const tm = "VOCE MORREU";
    const tw4: f64 = egui.measureText(win, tm, 46, 1);
    tituloSombra((W - tw4) / 2.0, 146, tm, 0xCC2233FF, 46);
    const res = "nivel " + nivel + "   |   " + pontos + " pontos   |   " + Math.round(profund) + "m de profundidade   |   seed " + seedRun;
    const rw: f64 = egui.measureText(win, res, 16, 0);
    egui.drawText(win, (W - rw) / 2.0, 224, res, 0xE0C0C8FF, 16, 0);
    const res2 = "almas guardadas no pickle: " + meta.almas;
    const rw2: f64 = egui.measureText(win, res2, 15, 0);
    egui.drawText(win, (W - rw2) / 2.0, 252, res2, 0xB090E0FF, 15, 0);
    if (botao(W / 2 - 160, 300, 320, 70, "VOLTAR A CRIPTA (R)", "", clicou, 1) === 1 || input.key(win, KEY_R, 1) != 0) {
      tela = T_INICIO;
    }
  }

  egui.endFrame(win);
}
salvarMeta();
io.print("[cripta] fim — almas " + meta.almas + " | melhor nivel " + meta.melhorNivel + " | mortes " + meta.totalMortes);
