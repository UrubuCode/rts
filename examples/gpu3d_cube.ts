// Demo do namespace gpu3d: cubo 3D indexado girando + UI egui por cima
// (label + botão), provando o scene pass sob o overlay.
// Rodar: target/release/rts.exe run examples/gpu3d_cube.ts
import { egui, gpu3d, buffer, time, io } from "rts";

const win: i64 = egui.openWindow("gpu3d — cubo", 900, 600, 0);

// ── Cubo: 8 vértices (x,y,z, r,g,b) + 36 índices (12 triângulos) ──────────
// Cada vértice com uma cor própria — o rasterizador interpola (prova o pipeline).
const verts: i64 = buffer.alloc(8 * 6 * 8);
function vtx(i: i64, x: f64, y: f64, z: f64, r: f64, g: f64, b: f64): void {
  const base: i64 = i * 48;
  buffer.write_f64(verts, base, x);
  buffer.write_f64(verts, base + 8, y);
  buffer.write_f64(verts, base + 16, z);
  buffer.write_f64(verts, base + 24, r);
  buffer.write_f64(verts, base + 32, g);
  buffer.write_f64(verts, base + 40, b);
}
vtx(0, -1, -1, -1, 1, 0, 0);
vtx(1, 1, -1, -1, 0, 1, 0);
vtx(2, 1, 1, -1, 0, 0, 1);
vtx(3, -1, 1, -1, 1, 1, 0);
vtx(4, -1, -1, 1, 1, 0, 1);
vtx(5, 1, -1, 1, 0, 1, 1);
vtx(6, 1, 1, 1, 1, 1, 1);
vtx(7, -1, 1, 1, 0.2, 0.2, 0.2);

const idx: i64 = buffer.alloc(36 * 4);
const faces: i64[] = [
  0, 1, 2, 0, 2, 3, // trás
  4, 6, 5, 4, 7, 6, // frente
  0, 4, 5, 0, 5, 1, // baixo
  3, 2, 6, 3, 6, 7, // cima
  0, 3, 7, 0, 7, 4, // esquerda
  1, 5, 6, 1, 6, 2, // direita
];
for (let i = 0; i < 36; i++) {
  buffer.write_i32(idx, i * 4, faces[i]);
}

const cube: i64 = gpu3d.meshIndexed(win, verts, 8, idx, 36);
buffer.free(verts);
buffer.free(idx);
io.print("meshId = " + cube);

gpu3d.clearColor(win, 0.07, 0.09, 0.13);
gpu3d.camera(win, 4, 3, 6, 0, 0, 0);
gpu3d.perspective(win, 60, 0.1, 100);

const t0: f64 = time.now_ms();
let frames: i64 = 0;
while (egui.isOpen(win)) {
  if (egui.pump(win) != 0) break;
  egui.beginFrame(win);

  const t: f64 = (time.now_ms() - t0) / 1000.0;
  // cubo central girando + dois satélites menores (3 draws, MVP por draw)
  gpu3d.draw(win, cube, 0, 0, 0, t * 50.0, t * 30.0, 1.0);
  gpu3d.draw(win, cube, 3.2, 0, 0, t * -80.0, 0, 0.5);
  gpu3d.draw(win, cube, -3.2, 0, 0, 0, t * 80.0, 0.5);

  egui.label(win, "gpu3d: cubo 3D real (WGSL + depth) sob o overlay egui");
  if (egui.button(win, "Fechar")) {
    egui.close(win);
    break;
  }

  egui.endFrame(win);
  frames = frames + 1;
  if (frames == 120) {
    io.print("120 frames renderizados ok");
  }
  if (frames >= 600) {
    egui.close(win);
    break;
  }
}
io.print("fim: " + frames + " frames");
