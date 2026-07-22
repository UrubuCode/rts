// Cenário estilo mapa de CS (pátio dust-like) no gpu3d: chão, muros de
// perímetro, paredes internas, caixas empilhadas, bombsite elevado, portas de
// metal e pilares — câmera orbitando por cima. Sem iluminação real: cada face
// da caixa tem um shade próprio (topo claro / laterais escuras), o truque
// clássico pré-lighting.
// Rodar: target/release/rts.exe run examples/gpu3d_cs_map.ts
import { egui, gpu3d, buffer, time, io } from "rts";

const win: i64 = egui.openWindow("gpu3d — de_rts (mapa estilo CS)", 1100, 700, 0);

// ── builder de caixa: base em y=0, centrada em x/z, shade por face ──────────
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
  // topo (shade 1.0)
  addV(vb, 0, -hw, h, -hd, r, g, b);
  addV(vb, 1, hw, h, -hd, r, g, b);
  addV(vb, 2, hw, h, hd, r, g, b);
  addV(vb, 3, -hw, h, hd, r, g, b);
  // fundo (0.45)
  addV(vb, 4, -hw, 0, -hd, r * 0.45, g * 0.45, b * 0.45);
  addV(vb, 5, hw, 0, -hd, r * 0.45, g * 0.45, b * 0.45);
  addV(vb, 6, hw, 0, hd, r * 0.45, g * 0.45, b * 0.45);
  addV(vb, 7, -hw, 0, hd, r * 0.45, g * 0.45, b * 0.45);
  // frente +z (0.85)
  addV(vb, 8, -hw, 0, hd, r * 0.85, g * 0.85, b * 0.85);
  addV(vb, 9, hw, 0, hd, r * 0.85, g * 0.85, b * 0.85);
  addV(vb, 10, hw, h, hd, r * 0.85, g * 0.85, b * 0.85);
  addV(vb, 11, -hw, h, hd, r * 0.85, g * 0.85, b * 0.85);
  // trás -z (0.78)
  addV(vb, 12, -hw, 0, -hd, r * 0.78, g * 0.78, b * 0.78);
  addV(vb, 13, hw, 0, -hd, r * 0.78, g * 0.78, b * 0.78);
  addV(vb, 14, hw, h, -hd, r * 0.78, g * 0.78, b * 0.78);
  addV(vb, 15, -hw, h, -hd, r * 0.78, g * 0.78, b * 0.78);
  // direita +x (0.70)
  addV(vb, 16, hw, 0, -hd, r * 0.7, g * 0.7, b * 0.7);
  addV(vb, 17, hw, 0, hd, r * 0.7, g * 0.7, b * 0.7);
  addV(vb, 18, hw, h, hd, r * 0.7, g * 0.7, b * 0.7);
  addV(vb, 19, hw, h, -hd, r * 0.7, g * 0.7, b * 0.7);
  // esquerda -x (0.62)
  addV(vb, 20, -hw, 0, -hd, r * 0.62, g * 0.62, b * 0.62);
  addV(vb, 21, -hw, 0, hd, r * 0.62, g * 0.62, b * 0.62);
  addV(vb, 22, -hw, h, hd, r * 0.62, g * 0.62, b * 0.62);
  addV(vb, 23, -hw, h, -hd, r * 0.62, g * 0.62, b * 0.62);
  // índices: 6 faces × 2 triângulos
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

// ── paleta dust ──────────────────────────────────────────────────────────────
const ground: i64 = boxMesh(44, 1, 44, 0.63, 0.58, 0.46); // areia escura
const wallX: i64 = boxMesh(44, 5, 1, 0.80, 0.71, 0.52);   // muro leste-oeste
const wallZ: i64 = boxMesh(1, 5, 44, 0.80, 0.71, 0.52);   // muro norte-sul
const midWall: i64 = boxMesh(16, 3.5, 1, 0.74, 0.64, 0.46);
const shortWall: i64 = boxMesh(8, 3.5, 1, 0.74, 0.64, 0.46);
const crate1: i64 = boxMesh(2, 2, 2, 0.56, 0.39, 0.20);   // caixa de madeira
const crate2: i64 = boxMesh(3, 3, 3, 0.50, 0.34, 0.17);   // caixa grande
const plat: i64 = boxMesh(11, 1.3, 11, 0.57, 0.51, 0.41); // bombsite A elevado
const doors: i64 = boxMesh(6, 4, 0.5, 0.34, 0.36, 0.40);  // portas duplas metal
const pillar: i64 = boxMesh(1.4, 4.5, 1.4, 0.71, 0.62, 0.45);

io.print("meshes: " + ground + ".." + pillar);

gpu3d.clearColor(win, 0.53, 0.68, 0.84); // céu
gpu3d.perspective(win, 62, 0.1, 200);

function drawMap(): void {
  // chão (afundado 1 pra o topo ficar em y=0) + perímetro
  gpu3d.draw(win, ground, 0, -1, 0, 0, 0, 1);
  gpu3d.draw(win, wallX, 0, 0, -22, 0, 0, 1);
  gpu3d.draw(win, wallX, 0, 0, 22, 0, 0, 1);
  gpu3d.draw(win, wallZ, -22, 0, 0, 0, 0, 1);
  gpu3d.draw(win, wallZ, 22, 0, 0, 0, 0, 1);
  // "mid": parede longa com vão + parede curta rotacionada (long A)
  gpu3d.draw(win, midWall, -6, 0, -4, 0, 0, 1);
  gpu3d.draw(win, shortWall, 10, 0, 6, 90, 0, 1);
  gpu3d.draw(win, shortWall, -14, 0, 10, 0, 0, 1);
  // bombsite A: plataforma elevada no canto + caixas em cima
  gpu3d.draw(win, plat, 14, 0, -14, 0, 0, 1);
  gpu3d.draw(win, crate1, 12, 1.3, -16, 0, 0, 1);
  gpu3d.draw(win, crate1, 16, 1.3, -13, 15, 0, 1);
  gpu3d.draw(win, crate1, 13.5, 3.3, -14.5, 40, 0, 1);
  // pilha de caixas no mid (cover clássico)
  gpu3d.draw(win, crate2, 1, 0, 5, 0, 0, 1);
  gpu3d.draw(win, crate1, -1.5, 0, 6, 25, 0, 1);
  gpu3d.draw(win, crate1, 0.5, 3, 5.5, 10, 0, 1);
  // bombsite B: caixas grandes no canto oposto
  gpu3d.draw(win, crate2, -16, 0, -15, 0, 0, 1);
  gpu3d.draw(win, crate2, -12.5, 0, -16.5, 30, 0, 1);
  gpu3d.draw(win, crate2, -14.5, 3, -15.5, 15, 0, 1);
  // portas duplas (T spawn) + porta lateral rotacionada
  gpu3d.draw(win, doors, -8, 0, 21.7, 0, 0, 1);
  gpu3d.draw(win, doors, 21.7, 0, 8, 90, 0, 1);
  // pilares do pátio central
  gpu3d.draw(win, pillar, 6, 0, -8, 0, 0, 1);
  gpu3d.draw(win, pillar, -6, 0, 12, 0, 0, 1);
  gpu3d.draw(win, pillar, 16, 0, 2, 0, 0, 1);
}

const t0: f64 = time.now_ms();
let frames: i64 = 0;
while (egui.isOpen(win)) {
  if (egui.pump(win) != 0) break;
  egui.beginFrame(win);

  // câmera orbitando o mapa por cima dos muros
  const t: f64 = (time.now_ms() - t0) / 1000.0;
  const ang: f64 = t * 0.35;
  const ex: f64 = Math.cos(ang) * 30.0;
  const ez: f64 = Math.sin(ang) * 30.0;
  const ey: f64 = 15.0 + Math.sin(t * 0.7) * 3.0;
  gpu3d.camera(win, ex, ey, ez, 0, 1, 0);

  drawMap();

  const fps: f64 = t > 0.2 ? frames / t : 0.0;
  egui.label(win, "de_rts — 23 draws/frame | " + Math.round(fps) + " fps");
  if (egui.button(win, "Fechar")) {
    egui.close(win);
    break;
  }

  egui.endFrame(win);
  frames = frames + 1;
  if (frames >= 1200) {
    egui.close(win);
    break;
  }
}
const dt: f64 = (time.now_ms() - t0) / 1000.0;
io.print("fim: " + frames + " frames em " + Math.round(dt * 10.0) / 10.0 + "s");
