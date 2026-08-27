import { GameObject } from "../engine/core";
import { World } from "../engine/world";

// RTS-MINE — Slime: mob que perambula quicando pelo terreno.

export class Slime extends GameObject {
  world: World;
  dirX: f64;
  dirZ: f64;
  timer: f64;
  age: f64;
  seed: number;
  lastCX: number;
  lastCZ: number;
  lastGY: number;
  acc: f64;

  constructor(world: World, x: number, z: number, seed: number) {
    super("Slime");
    this.world = world;
    this.transform.x = x;
    this.transform.y = 30.0;
    this.transform.z = z;
    this.dirX = 1.0;
    this.dirZ = 0.0;
    this.timer = 0.2 + seed * 0.4;
    this.age = seed * 1.3;
    this.seed = seed;
    this.spriteKind = 1;
    this.spriteSize = 0.85;
    this.lastCX = -1;
    this.lastCZ = -1;
    this.lastGY = -1;
    this.acc = 0.0;
  }

  update(dt: f64): void {
    this.acc = this.acc + dt;
    if (this.acc < 0.028) return;
    const dtE = this.acc;
    this.acc = 0.0;
    const w = this.world;
    const t = this.transform;
    this.age = this.age + dtE;
    this.timer = this.timer - dtE;
    if (this.timer < 0) {
      const hh = Math.sin(t.x * 12.9898 + t.z * 78.233 + this.seed * 3.77) * 43758.5453;
      const r = hh - Math.floor(hh);
      const ang = r * 6.28318;
      this.dirX = Math.cos(ang);
      this.dirZ = Math.sin(ang);
      this.timer = 1.2 + r * 2.5;
    }

    let nx: f64 = t.x + this.dirX * 1.8 * dtE;
    let nz: f64 = t.z + this.dirZ * 1.8 * dtE;
    if (nx < 2) { nx = 2; this.timer = 0; }
    if (nx > w.sx - 3) { nx = w.sx - 3; this.timer = 0; }
    if (nz < 2) { nz = 2; this.timer = 0; }
    if (nz > w.sz - 3) { nz = w.sz - 3; this.timer = 0; }
    const cellX = Math.floor(nx);
    const cellZ = Math.floor(nz);
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
      const hop = Math.abs(Math.sin(this.age * 3.2)) * 0.35;
      t.y = gy + 1 + 0.42 + hop;
    } else {
      this.timer = 0;
      this.lastCX = -1;
    }
  }
}
