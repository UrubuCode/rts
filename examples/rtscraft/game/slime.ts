import math from "rts:math";

import { GameObject } from "../engine/core";
import { World } from "../engine/world";

// RTS-MINE — Slime: mob que perambula quicando pelo terreno. Estilo Minecraft,
// com renderer PRÓPRIO: a entidade não vive no grid de voxels — o
// entity-renderer (engine/render3d.ts) projeta um billboard animado no
// framebuffer. Toda a IA vive no update(dt) da classe.

export class Slime extends GameObject {
  world: World;
  dirX: f64;
  dirZ: f64;
  timer: f64;
  age: f64;
  seed: number;
  // cache do terreno: só re-escaneia a altura quando muda de célula
  lastCX: number;
  lastCZ: number;
  lastGY: number;
  acc: f64; // acumulador de dt (IA a ~30Hz)

  constructor(world: World, x: number, z: number, seed: number) {
    super("Slime");
    this.world = world;
    this.transform.x = x;
    this.transform.y = 30.0; // assenta no chão no 1º update
    this.transform.z = z;
    this.dirX = 1.0;
    this.dirZ = 0.0;
    this.timer = 0.2 + seed * 0.4;
    this.age = seed * 1.3;
    this.seed = seed;
    this.spriteKind = 1;  // billboard de slime
    this.spriteSize = 0.85;
    this.lastCX = -1;
    this.lastCZ = -1;
    this.lastGY = -1;
    this.acc = 0.0;
  }

  update(dt: f64): void {
    // IA a ~30Hz: acumula o dt e roda frame sim, frame não (mobs não precisam
    // de 60Hz; corta o custo de update pela metade)
    this.acc = this.acc + dt;
    if (this.acc < 0.028) return;
    const dtE = this.acc;
    this.acc = 0.0;
    const w = this.world;
    const t = this.transform;
    this.age = this.age + dtE;

    // troca de direção periodicamente (hash determinístico)
    this.timer = this.timer - dt;
    if (this.timer < 0) {
      const hh = math.sin(t.x * 12.9898 + t.z * 78.233 + this.seed * 3.77) * 43758.5453;
      const r = hh - math.floor(hh);
      const ang = r * 6.28318;
      this.dirX = math.cos(ang);
      this.dirZ = math.sin(ang);
      this.timer = 1.2 + r * 2.5;
    }

    // anda seguindo o terreno (não entra em água funda nem sai do mapa)
    let nx: f64 = t.x + this.dirX * 1.8 * dt;
    let nz: f64 = t.z + this.dirZ * 1.8 * dt;
    if (nx < 2) { nx = 2; this.timer = 0; }
    if (nx > w.sx - 3) { nx = w.sx - 3; this.timer = 0; }
    if (nz < 2) { nz = 2; this.timer = 0; }
    if (nz > w.sz - 3) { nz = w.sz - 3; this.timer = 0; }
    // altura do terreno com CACHE por célula (topY é caro; o slime só muda de
    // célula ~2×/s — sem cache eram ~30 leituras por frame POR slime)
    const cellX = math.floor(nx);
    const cellZ = math.floor(nz);
    let gy = this.lastGY;
    if (cellX !== this.lastCX || cellZ !== this.lastCZ) {
      gy = w.topY(cellX, cellZ);
      this.lastCX = cellX;
      this.lastCZ = cellZ;
      this.lastGY = gy;
    }
    if (gy >= 0 && gy + 1 < w.sy && gy + 1 > w.waterLevel) {
      t.x = nx;
      t.z = nz;
      // centro do corpo: em cima do chão + pulinho (hop) animado
      const hop = math.abs(math.sin(this.age * 3.2)) * 0.35;
      t.y = gy + 1 + 0.42 + hop;
    } else {
      this.timer = 0;     // sem chão utilizável: muda de direção
      this.lastCX = -1;   // invalida o cache (célula rejeitada)
    }
  }
}
