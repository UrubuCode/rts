// CRISTAIS 3D — jogo-demo do PICKLE (rts:serde) + scene pass 3D (egui/wgpu).
//
//   Colete os cristais girantes da arena sob o céu estrelado. Cada nível
//   spawna mais cristais. O estado INTEIRO do jogo (posição, câmera, score,
//   nível, cristais restantes — instâncias de classe!) é picklado com
//   rts:serde num arquivo .rtsp:
//
//   W/A/S/D — andar      ESPAÇO — pular      MOUSE — olhar (captura FPS)
//   1 — SALVAR (serialize → claude-cristais-save.rtsp)
//   2 — CARREGAR (deserialize; também carrega sozinho no boot)
//   ESC — solta/recaptura o mouse      Q — sair
//
//   Rodar: target/release/rts.exe run examples/claude-cristais3d.ts
import { egui, input, buffer, time, io } from "rts";
import { serialize, deserialize } from "rts:serde";
import { writeFileSync, readFileSync, existsSync } from "node:fs";

const win: i64 = egui.openWindow("CRISTAIS 3D — pickle demo (rts:serde + wgpu)", 1100, 700, 0);

// ── ESTADO PICKLÁVEL (classes de verdade — o ponto da demo) ─────────────────
class Cristal {
  x: f64;
  z: f64;
  cor: number;   // 0..4 → paleta (valor = cor+1)
  valor: number;
  fase: f64;     // offset de giro/flutuação
  constructor(x: f64, z: f64, cor: number, valor: number, fase: f64) {
    this.x = x;
    this.z = z;
    this.cor = cor;
    this.valor = valor;
    this.fase = fase;
  }
}

class SaveGame {
  px: f64;
  pz: f64;
  yaw: f64;
  pitch: f64;
  score: number;
  nivel: number;
  tempo: f64;
  cristais: Cristal[];
  constructor() {
    this.px = 0.0;
    this.pz = -12.0;
    this.yaw = 0.0;
    this.pitch = 0.0;
    this.score = 0;
    this.nivel = 1;
    this.tempo = 0.0;
    this.cristais = [];
  }
}

const SAVE_PATH = "claude-cristais-save.rtsp";

// ── MALHA: um cubo unitário branco (cor vem por draw) ───────────────────────
// 24 vértices × 8 f32 (pos xyz + normal xyz + uv) + 36 índices u32.
function cubeMesh(): i64 {
  const vb: i64 = buffer.alloc(24 * 32);
  const ib: i64 = buffer.alloc(36 * 4);
  let vi = 0;
  // por face: eixo da normal (nx,ny,nz) e os 4 cantos
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
  const vp: i64 = buffer.ptr(vb);
  const ip: i64 = buffer.ptr(ib);
  const id: i64 = egui.meshUpload(win, vp, 24, ip, 36);
  buffer.free(vb);
  buffer.free(ib);
  return id;
}

const cube: i64 = cubeMesh();
io.print("[cristais] mesh id " + cube);

// luz PONTO alta sobre a arena + sombra direcional suave
egui.setLight(win, 0, 16, 0, 0.38);
egui.setShadow(win, 0.35, -1.0, 0.25, 0, 0, 0, 32);

// paleta 0xAARRGGBB (valor cresce com a cor)
const CORES: number[] = [0xFF4DD8F2, 0xFF59E667, 0xFFF2D84D, 0xFFF27540, 0xFFD95CE6];

// ── estado vivo ─────────────────────────────────────────────────────────────
let save = new SaveGame();

function spawnNivel(n: number): Cristal[] {
  const lista: Cristal[] = [];
  const qtd = 4 + n * 3;
  for (let i = 0; i < qtd; i++) {
    const x: f64 = (Math.random() - 0.5) * 34.0;
    const z: f64 = (Math.random() - 0.5) * 34.0;
    const cor = Math.floor(Math.random() * 5);
    lista.push(new Cristal(x, z, cor, cor + 1, Math.random() * 6.28));
  }
  return lista;
}

let toast = "";
let toastT: f64 = 0.0;
// boot: continua o save se existir (deserialize → instanceof SaveGame)
if (existsSync(SAVE_PATH)) {
  const carregado: any = deserialize(readFileSync(SAVE_PATH) as any);
  if (carregado instanceof SaveGame) {
    save = carregado;
    toast = "SAVE CARREGADO — nivel " + save.nivel + ", score " + save.score;
    toastT = 4.0;
    io.print("[cristais] save carregado: nivel " + save.nivel + " score " + save.score + " cristais " + save.cristais.length);
  }
}
if (save.cristais.length === 0) {
  save.cristais = spawnNivel(save.nivel);
}

let py: f64 = 0.0;
let vy: f64 = 0.0;

const KEY_W: i64 = 122;
const KEY_A: i64 = 100;
const KEY_S: i64 = 118;
const KEY_D: i64 = 103;
const KEY_SPACE: i64 = 3;
const KEY_ESC: i64 = 2;
const KEY_UP: i64 = 5;
const KEY_DOWN: i64 = 6;
const KEY_LEFT: i64 = 7;
const KEY_RIGHT: i64 = 8;
const KEY_1: i64 = 131;
const KEY_2: i64 = 132;
const KEY_Q: i64 = 116; // 'q' na convenção ascii+3 do backend (a=100, s=118)

// captura FPS do mouse (pointer-lock): olhar vem do delta cru do device
let mouseCapturado = 1;
egui.mouseLock(win, 1);

function salvar(): void {
  writeFileSync(SAVE_PATH, serialize(save) as any);
  toast = "SALVO em " + SAVE_PATH + " (" + save.cristais.length + " cristais no pickle)";
  toastT = 3.0;
  io.print("[cristais] salvo: nivel " + save.nivel + " score " + save.score);
}

function carregar(): void {
  if (existsSync(SAVE_PATH) === false) {
    toast = "NADA PRA CARREGAR (salve com 1 primeiro)";
    toastT = 3.0;
    return;
  }
  const c: any = deserialize(readFileSync(SAVE_PATH) as any);
  if (c instanceof SaveGame) {
    save = c;
    toast = "CARREGADO — nivel " + save.nivel + ", score " + save.score;
    toastT = 3.0;
  }
}

io.print("[cristais] pronto: WASD anda, ESPACO pula, setas/dir+mouse olham, 1 salva, 2 carrega, ESC sai");

const t0: f64 = time.now_ms();
let last: f64 = t0;
let frames: i64 = 0;
while (egui.isOpen(win)) {
  if (egui.pump(win) != 0) break;
  egui.beginFrame(win);

  const now: f64 = time.now_ms();
  let dt: f64 = (now - last) / 1000.0;
  last = now;
  if (dt > 0.1) {
    dt = 0.1;
  }
  save.tempo = save.tempo + dt;

  // ── olhar (base LH: yaw 0 = +Z; fwd = sin/cos) ───────────────────────────
  const lookSpd: f64 = 2.2 * dt;
  if (input.key(win, KEY_LEFT, 0) != 0) { save.yaw = save.yaw - lookSpd; }
  if (input.key(win, KEY_RIGHT, 0) != 0) { save.yaw = save.yaw + lookSpd; }
  if (input.key(win, KEY_UP, 0) != 0) { save.pitch = save.pitch + lookSpd; }
  if (input.key(win, KEY_DOWN, 0) != 0) { save.pitch = save.pitch - lookSpd; }
  if (mouseCapturado === 1) {
    save.yaw = save.yaw + input.mouseDeltaX(win) * 0.0035;
    save.pitch = save.pitch - input.mouseDeltaY(win) * 0.0035;
  }
  save.pitch = Math.max(-1.25, Math.min(1.25, save.pitch));

  // ── andar (fwd LH: x=sin(yaw), z=cos(yaw); right: x=cos, z=-sin) ─────────
  const spd: f64 = 7.0;
  const fx: f64 = Math.sin(save.yaw);
  const fz: f64 = Math.cos(save.yaw);
  const rx2: f64 = Math.cos(save.yaw);
  const rz2: f64 = -Math.sin(save.yaw);
  let mx: f64 = 0.0;
  let mz: f64 = 0.0;
  if (input.key(win, KEY_W, 0) != 0) { mx = mx + fx; mz = mz + fz; }
  if (input.key(win, KEY_S, 0) != 0) { mx = mx - fx; mz = mz - fz; }
  if (input.key(win, KEY_D, 0) != 0) { mx = mx + rx2; mz = mz + rz2; }
  if (input.key(win, KEY_A, 0) != 0) { mx = mx - rx2; mz = mz - rz2; }
  const ml: f64 = Math.sqrt(mx * mx + mz * mz);
  if (ml > 0.001) {
    save.px = save.px + (mx / ml) * spd * dt;
    save.pz = save.pz + (mz / ml) * spd * dt;
  }
  save.px = Math.max(-19.0, Math.min(19.0, save.px));
  save.pz = Math.max(-19.0, Math.min(19.0, save.pz));

  // ── pulo ──────────────────────────────────────────────────────────────────
  const grounded: boolean = py <= 0.001;
  if (grounded && input.key(win, KEY_SPACE, 0) != 0) {
    vy = 5.2;
  }
  vy = vy - 14.0 * dt;
  py = py + vy * dt;
  if (py < 0.0) {
    py = 0.0;
    vy = 0.0;
  }

  // ── COLETA ────────────────────────────────────────────────────────────────
  let ci = 0;
  while (ci < save.cristais.length) {
    const c = save.cristais[ci];
    const dx: f64 = save.px - c.x;
    const dz: f64 = save.pz - c.z;
    if (dx * dx + dz * dz < 1.69) {
      save.score = save.score + c.valor;
      // swap-remove (o último ocupa a vaga; pop encolhe)
      save.cristais[ci] = save.cristais[save.cristais.length - 1];
      save.cristais.pop();
      toast = "+ cristal! score " + save.score;
      toastT = 1.2;
    } else {
      ci = ci + 1;
    }
  }
  if (save.cristais.length === 0) {
    save.nivel = save.nivel + 1;
    save.cristais = spawnNivel(save.nivel);
    toast = "NIVEL " + save.nivel + "!  (" + save.cristais.length + " cristais)";
    toastT = 3.0;
  }

  // ── SAVE / LOAD via pickle ────────────────────────────────────────────────
  if (input.key(win, KEY_1, 1) != 0) { salvar(); }
  if (input.key(win, KEY_2, 1) != 0) { carregar(); }
  if (input.key(win, KEY_ESC, 1) != 0) {
    // ESC alterna a captura do mouse (soltar pra usar outras janelas)
    if (mouseCapturado === 1) {
      mouseCapturado = 0;
      egui.mouseLock(win, 0);
      toast = "mouse SOLTO — ESC recaptura, Q sai";
      toastT = 2.5;
    } else {
      mouseCapturado = 1;
      egui.mouseLock(win, 1);
    }
  }
  if (input.key(win, KEY_Q, 1) != 0) {
    egui.close(win);
    break;
  }

  // ── câmera fly (radianos; aspect segue resize) ───────────────────────────
  const ey: f64 = py + 1.7;
  const aspect: f64 = egui.winWidth(win) / egui.winHeight(win);
  egui.setCamera(win, save.px, ey, save.pz, save.yaw, save.pitch, 1.22, aspect);

  // ── mundo (um cubo, escalas/cores por draw) ──────────────────────────────
  // chão
  egui.drawMesh(win, cube, 0, -0.5, 0, 0, 0, 40, 1, 40, 0xFF3E5A44, 0, 1);
  // muros do perímetro
  egui.drawMesh(win, cube, 0, 1.5, -20, 0, 0, 40, 3, 1, 0xFF5A5670, 0, 0);
  egui.drawMesh(win, cube, 0, 1.5, 20, 0, 0, 40, 3, 1, 0xFF5A5670, 0, 0);
  egui.drawMesh(win, cube, -20, 1.5, 0, 0, 0, 1, 3, 40, 0xFF5A5670, 0, 0);
  egui.drawMesh(win, cube, 20, 1.5, 0, 0, 0, 1, 3, 40, 0xFF5A5670, 0, 0);
  // pedras de cenário
  egui.drawMesh(win, cube, -8, 0.8, -8, 0, 0.35, 2.2, 1.6, 2.2, 0xFF615C57, 0, 0);
  egui.drawMesh(win, cube, 9, 0.8, 5, 0, 1.1, 2.2, 1.6, 2.2, 0xFF615C57, 0, 0);
  egui.drawMesh(win, cube, -4, 0.8, 11, 0, 0.7, 2.2, 1.6, 2.2, 0xFF615C57, 0, 0);

  // cristais: giram (2 eixos) e flutuam — EMISSIVOS, brilham no escuro
  const tt: f64 = save.tempo;
  for (let i = 0; i < save.cristais.length; i++) {
    const c = save.cristais[i];
    const cy: f64 = 0.9 + Math.sin(tt * 2.0 + c.fase) * 0.25;
    const rot: f64 = tt * 1.7 + c.fase;
    egui.drawMesh(win, cube, c.x, cy, c.z, 0.62, rot, 0.55, 0.9, 0.55, CORES[c.cor], 1, 0);
  }

  // ── HUD ───────────────────────────────────────────────────────────────────
  const cx0: f64 = egui.winWidth(win) / 2.0;
  const cy0: f64 = egui.winHeight(win) / 2.0;
  egui.drawLine(win, cx0 - 10, cy0, cx0 + 10, cy0, 2, 0xFFFFFFAA);
  egui.drawLine(win, cx0, cy0 - 10, cx0, cy0 + 10, 2, 0xFFFFFFAA);
  const t: f64 = (now - t0) / 1000.0;
  const fps: f64 = t > 0.2 ? frames / t : 0.0;
  egui.label(win, "CRISTAIS 3D | score " + save.score + " | nivel " + save.nivel + " | faltam " + save.cristais.length + " | " + Math.round(fps) + " fps");
  egui.label(win, "WASD anda | ESPACO pula | MOUSE olha | [1] SALVAR pickle | [2] CARREGAR | ESC solta mouse | Q sai");
  if (toastT > 0.0) {
    toastT = toastT - dt;
    egui.label(win, ">> " + toast);
  }

  egui.endFrame(win);
  frames = frames + 1;
}
io.print("[cristais] fim: score " + save.score + " nivel " + save.nivel + " tempo " + Math.round(save.tempo) + "s");
