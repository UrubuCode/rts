// JOGO teste em primeira pessoa no mapa estilo CS (gpu3d):
//   W/A/S/D  — andar          SHIFT — correr
//   ESPAÇO   — pular           SETAS — virar a câmera
//   BOTÃO DIREITO segurado + mouse — olhar (mouse-look estilo editor)
//   ESC      — sair            clique ESQUERDO — "atirar" (contador)
// Colisão: perímetro + caixas/muros/pilares (empurra no plano XZ);
// plataforma do bombsite A é pisável (chão elevado).
// Rodar: target/release/rts.exe run examples/gpu3d_fps_game.ts
import { egui, gpu3d, input, buffer, time, io } from "rts";

const win: i64 = egui.openWindow("de_rts — FPS teste (gpu3d)", 1100, 700, 0);

// ── builder de caixa (base y=0, shade por face) ─────────────────────────────
function addV(vb: i64, i: i64, x: f64, y: f64, z: f64, r: f64, g: f64, b: f64): void {
  const base: i64 = i * 48;
  buffer.write_f64(vb, base, x);
  buffer.write_f64(vb, base + 8, y);
  buffer.write_f64(vb, base + 16, z);
  buffer.write_f64(vb, base + 24, r);
  buffer.write_f64(vb, base + 32, g);
  buffer.write_f64(vb, base + 40, b);
}

function boxMesh(w: f64, h: f64, d: f64, r: f64, g: f64, b: f64): i64 {
  const hw: f64 = w / 2.0;
  const hd: f64 = d / 2.0;
  const vb: i64 = buffer.alloc(24 * 6 * 8);
  addV(vb, 0, -hw, h, -hd, r, g, b);
  addV(vb, 1, hw, h, -hd, r, g, b);
  addV(vb, 2, hw, h, hd, r, g, b);
  addV(vb, 3, -hw, h, hd, r, g, b);
  addV(vb, 4, -hw, 0, -hd, r * 0.45, g * 0.45, b * 0.45);
  addV(vb, 5, hw, 0, -hd, r * 0.45, g * 0.45, b * 0.45);
  addV(vb, 6, hw, 0, hd, r * 0.45, g * 0.45, b * 0.45);
  addV(vb, 7, -hw, 0, hd, r * 0.45, g * 0.45, b * 0.45);
  addV(vb, 8, -hw, 0, hd, r * 0.85, g * 0.85, b * 0.85);
  addV(vb, 9, hw, 0, hd, r * 0.85, g * 0.85, b * 0.85);
  addV(vb, 10, hw, h, hd, r * 0.85, g * 0.85, b * 0.85);
  addV(vb, 11, -hw, h, hd, r * 0.85, g * 0.85, b * 0.85);
  addV(vb, 12, -hw, 0, -hd, r * 0.78, g * 0.78, b * 0.78);
  addV(vb, 13, hw, 0, -hd, r * 0.78, g * 0.78, b * 0.78);
  addV(vb, 14, hw, h, -hd, r * 0.78, g * 0.78, b * 0.78);
  addV(vb, 15, -hw, h, -hd, r * 0.78, g * 0.78, b * 0.78);
  addV(vb, 16, hw, 0, -hd, r * 0.7, g * 0.7, b * 0.7);
  addV(vb, 17, hw, 0, hd, r * 0.7, g * 0.7, b * 0.7);
  addV(vb, 18, hw, h, hd, r * 0.7, g * 0.7, b * 0.7);
  addV(vb, 19, hw, h, -hd, r * 0.7, g * 0.7, b * 0.7);
  addV(vb, 20, -hw, 0, -hd, r * 0.62, g * 0.62, b * 0.62);
  addV(vb, 21, -hw, 0, hd, r * 0.62, g * 0.62, b * 0.62);
  addV(vb, 22, -hw, h, hd, r * 0.62, g * 0.62, b * 0.62);
  addV(vb, 23, -hw, h, -hd, r * 0.62, g * 0.62, b * 0.62);
  const ib: i64 = buffer.alloc(36 * 4);
  for (let f = 0; f < 6; f++) {
    const v: i64 = f * 4;
    const o: i64 = f * 24;
    buffer.write_i32(ib, o, v);
    buffer.write_i32(ib, o + 4, v + 1);
    buffer.write_i32(ib, o + 8, v + 2);
    buffer.write_i32(ib, o + 12, v);
    buffer.write_i32(ib, o + 16, v + 2);
    buffer.write_i32(ib, o + 20, v + 3);
  }
  const id: i64 = gpu3d.meshIndexed(win, vb, 24, ib, 36);
  buffer.free(vb);
  buffer.free(ib);
  return id;
}

// ── malhas ───────────────────────────────────────────────────────────────────
const ground: i64 = boxMesh(44, 1, 44, 0.63, 0.58, 0.46);
const wallX: i64 = boxMesh(44, 5, 1, 0.80, 0.71, 0.52);
const wallZ: i64 = boxMesh(1, 5, 44, 0.80, 0.71, 0.52);
const midWall: i64 = boxMesh(16, 3.5, 1, 0.74, 0.64, 0.46);
const shortWall: i64 = boxMesh(8, 3.5, 1, 0.74, 0.64, 0.46);
const crate1: i64 = boxMesh(2, 2, 2, 0.56, 0.39, 0.20);
const crate2: i64 = boxMesh(3, 3, 3, 0.50, 0.34, 0.17);
const plat: i64 = boxMesh(11, 1.3, 11, 0.57, 0.51, 0.41);
const doors: i64 = boxMesh(6, 4, 0.5, 0.34, 0.36, 0.40);
const pillar: i64 = boxMesh(1.4, 4.5, 1.4, 0.71, 0.62, 0.45);

gpu3d.clearColor(win, 0.53, 0.68, 0.84);
gpu3d.perspective(win, 70, 0.08, 200);

// ── colisores AABB no plano XZ (paralelos: minX, maxX, minZ, maxZ) ──────────
// Rotações de deco aproximadas pelo AABB folgado. Plataforma NÃO entra aqui
// (é pisável — vira chão elevado), caixas/muros/pilares empurram.
const colMinX: f64[] = [];
const colMaxX: f64[] = [];
const colMinZ: f64[] = [];
const colMaxZ: f64[] = [];
function addCol(cx: f64, cz: f64, w: f64, d: f64): void {
  colMinX.push(cx - w / 2.0);
  colMaxX.push(cx + w / 2.0);
  colMinZ.push(cz - d / 2.0);
  colMaxZ.push(cz + d / 2.0);
}
addCol(-6, -4, 16, 1);      // midWall
addCol(10, 6, 1, 8);        // shortWall rot 90
addCol(-14, 10, 8, 1);      // shortWall 2
addCol(12, -16, 2.4, 2.4);  // caixas do site A (sobre a plataforma)
addCol(16, -13, 2.4, 2.4);
addCol(1, 5, 3, 3);         // pilha mid
addCol(-1.5, 6, 2.4, 2.4);
addCol(-16, -15, 3, 3);     // site B
addCol(-12.5, -16.5, 3.4, 3.4);
addCol(-8, 21.7, 6, 0.5);   // portas
addCol(21.7, 8, 0.5, 6);
addCol(6, -8, 1.4, 1.4);    // pilares
addCol(-6, 12, 1.4, 1.4);
addCol(16, 2, 1.4, 1.4);

function drawMap(): void {
  gpu3d.draw(win, ground, 0, -1, 0, 0, 0, 1);
  gpu3d.draw(win, wallX, 0, 0, -22, 0, 0, 1);
  gpu3d.draw(win, wallX, 0, 0, 22, 0, 0, 1);
  gpu3d.draw(win, wallZ, -22, 0, 0, 0, 0, 1);
  gpu3d.draw(win, wallZ, 22, 0, 0, 0, 0, 1);
  gpu3d.draw(win, midWall, -6, 0, -4, 0, 0, 1);
  gpu3d.draw(win, shortWall, 10, 0, 6, 90, 0, 1);
  gpu3d.draw(win, shortWall, -14, 0, 10, 0, 0, 1);
  gpu3d.draw(win, plat, 14, 0, -14, 0, 0, 1);
  gpu3d.draw(win, crate1, 12, 1.3, -16, 0, 0, 1);
  gpu3d.draw(win, crate1, 16, 1.3, -13, 15, 0, 1);
  gpu3d.draw(win, crate1, 13.5, 3.3, -14.5, 40, 0, 1);
  gpu3d.draw(win, crate2, 1, 0, 5, 0, 0, 1);
  gpu3d.draw(win, crate1, -1.5, 0, 6, 25, 0, 1);
  gpu3d.draw(win, crate1, 0.5, 3, 5.5, 10, 0, 1);
  gpu3d.draw(win, crate2, -16, 0, -15, 0, 0, 1);
  gpu3d.draw(win, crate2, -12.5, 0, -16.5, 30, 0, 1);
  gpu3d.draw(win, crate2, -14.5, 3, -15.5, 15, 0, 1);
  gpu3d.draw(win, doors, -8, 0, 21.7, 0, 0, 1);
  gpu3d.draw(win, doors, 21.7, 0, 8, 90, 0, 1);
  gpu3d.draw(win, pillar, 6, 0, -8, 0, 0, 1);
  gpu3d.draw(win, pillar, -6, 0, 12, 0, 0, 1);
  gpu3d.draw(win, pillar, 16, 0, 2, 0, 0, 1);
}

// ── estado do jogador ────────────────────────────────────────────────────────
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

let px: f64 = 0.0;
let pz: f64 = 16.0;   // nasce perto do "T spawn" (portas ao sul)
let py: f64 = 0.0;    // altura dos PÉS
let vy: f64 = 0.0;
let yaw: f64 = 0.0;  // yaw 0 = olhando pro norte (-z, centro do mapa)
let pitch: f64 = 0.0;
let shots: i64 = 0;
const EYE: f64 = 1.7;
const RADIUS: f64 = 0.45;

// chão local: plataforma do site A (topo 1.3) é pisável; resto 0.
function floorAt(x: f64, z: f64): f64 {
  if (x > 8.5 && x < 19.5 && z > -19.5 && z < -8.5) {
    return 1.3;
  }
  return 0.0;
}

// empurra o jogador pra fora dos colisores (círculo vs AABB, eixo de menor
// penetração) — só quando os pés estão abaixo do topo do obstáculo (aprox 3m).
function collide(): void {
  for (let i = 0; i < colMinX.length; i++) {
    const nx: f64 = Math.max(colMinX[i], Math.min(px, colMaxX[i]));
    const nz: f64 = Math.max(colMinZ[i], Math.min(pz, colMaxZ[i]));
    const dx: f64 = px - nx;
    const dz: f64 = pz - nz;
    const d2: f64 = dx * dx + dz * dz;
    if (d2 < RADIUS * RADIUS) {
      if (d2 > 0.000001) {
        const d: f64 = Math.sqrt(d2);
        px = nx + (dx / d) * RADIUS;
        pz = nz + (dz / d) * RADIUS;
      } else {
        // centro dentro do AABB: expulsa pelo eixo de menor penetração
        const lx: f64 = Math.min(px - colMinX[i], colMaxX[i] - px);
        const lz: f64 = Math.min(pz - colMinZ[i], colMaxZ[i] - pz);
        if (lx < lz) {
          px = px - colMinX[i] < colMaxX[i] - px ? colMinX[i] - RADIUS : colMaxX[i] + RADIUS;
        } else {
          pz = pz - colMinZ[i] < colMaxZ[i] - pz ? colMinZ[i] - RADIUS : colMaxZ[i] + RADIUS;
        }
      }
    }
  }
  // perímetro (muros)
  px = Math.max(-21.0, Math.min(21.0, px));
  pz = Math.max(-21.0, Math.min(21.0, pz));
}

io.print("de_rts FPS: WASD anda, SHIFT corre, ESPACO pula, setas/botao-direito olham, ESC sai");

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
    dt = 0.1; // clamp em stall (arrasto de janela etc)
  }

  // ── olhar: setas sempre; mouse-look com botão DIREITO segurado ────────────
  const lookSpd: f64 = 2.2 * dt;
  if (input.key(win, KEY_LEFT, 0) != 0) { yaw = yaw - lookSpd; }
  if (input.key(win, KEY_RIGHT, 0) != 0) { yaw = yaw + lookSpd; }
  if (input.key(win, KEY_UP, 0) != 0) { pitch = pitch + lookSpd; }
  if (input.key(win, KEY_DOWN, 0) != 0) { pitch = pitch - lookSpd; }
  if (input.mouseDown(win, 1) != 0) {
    yaw = yaw + input.mouseDeltaX(win) * 0.0035;
    pitch = pitch - input.mouseDeltaY(win) * 0.0035;
  }
  pitch = Math.max(-1.25, Math.min(1.25, pitch));

  // ── andar (relativo ao yaw, no plano) ─────────────────────────────────────
  const run: f64 = input.modShift(win) != 0 ? 9.5 : 5.5;
  const fx: f64 = Math.sin(yaw);
  const fz: f64 = -Math.cos(yaw);
  let mx: f64 = 0.0;
  let mz: f64 = 0.0;
  if (input.key(win, KEY_W, 0) != 0) { mx = mx + fx; mz = mz + fz; }
  if (input.key(win, KEY_S, 0) != 0) { mx = mx - fx; mz = mz - fz; }
  if (input.key(win, KEY_A, 0) != 0) { mx = mx + fz; mz = mz - fx; }
  if (input.key(win, KEY_D, 0) != 0) { mx = mx - fz; mz = mz + fx; }
  const ml: f64 = Math.sqrt(mx * mx + mz * mz);
  if (ml > 0.001) {
    px = px + (mx / ml) * run * dt;
    pz = pz + (mz / ml) * run * dt;
  }
  collide();

  // ── pulo + gravidade ──────────────────────────────────────────────────────
  const fl: f64 = floorAt(px, pz);
  const grounded: boolean = py <= fl + 0.001;
  if (grounded && input.key(win, KEY_SPACE, 0) != 0) {
    vy = 5.2;
  }
  vy = vy - 14.0 * dt;
  py = py + vy * dt;
  if (py < fl) {
    py = fl;
    vy = 0.0;
  }

  // ── "atirar" (só contador por enquanto) + sair ────────────────────────────
  if (input.mouseClicked(win, 0) != 0) {
    shots = shots + 1;
  }
  if (input.key(win, KEY_ESC, 1) != 0) {
    egui.close(win);
    break;
  }

  // ── câmera primeira pessoa ────────────────────────────────────────────────
  const ey: f64 = py + EYE;
  const cp: f64 = Math.cos(pitch);
  gpu3d.camera(win, px, ey, pz, px + fx * cp, ey + Math.sin(pitch), pz + fz * cp);

  drawMap();

  // ── HUD: crosshair (canvas 2D burro do egui) + status ────────────────────
  egui.drawLine(win, 540, 350, 560, 350, 2, 0xFFFFFFCC);
  egui.drawLine(win, 550, 340, 550, 360, 2, 0xFFFFFFCC);
  const t: f64 = (now - t0) / 1000.0;
  const fps: f64 = t > 0.2 ? frames / t : 0.0;
  egui.label(win, "WASD anda | SHIFT corre | ESPACO pula | dir.+mouse olha | ESC sai");
  egui.label(win, Math.round(fps) + " fps | tiros: " + shots + " | pos " + Math.round(px) + "," + Math.round(pz));

  egui.endFrame(win);
  frames = frames + 1;
}
io.print("fim: " + frames + " frames, " + shots + " tiros");
