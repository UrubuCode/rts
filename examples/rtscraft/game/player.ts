import math from "rts:math";

import { GameObject } from "../engine/core";
import { World } from "../engine/world";
import { Input } from "../engine/input";

// RTS-MINE — o controlador do jogador, estilo Unity: um GameObject cujo
// update(dt) lê Input, move o Transform e resolve física contra o World.

export class PlayerController extends GameObject {
  world: World;
  input: Input;
  vy: f64;
  grounded: number;
  inWater: number;
  speed: f64;
  lookSens: f64;

  constructor(world: World, inp: Input) {
    super("Player");
    this.world = world;
    this.input = inp;
    this.vy = 0.0;
    this.grounded = 0;
    this.inWater = 0;
    this.speed = 5.4;
    this.lookSens = 0.0038;
  }

  /// Posiciona no topo do terreno em (x,z), olhando pra frente.
  spawnAt(x: number, z: number): void {
    const top = this.world.topY(x, z);
    this.transform.x = x + 0.5;
    this.transform.y = top + 2.7;
    this.transform.z = z + 0.5;
    this.transform.yaw = 0.7;
    this.transform.pitch = 0 - 0.1;
  }

  update(dt: f64): void {
    const t = this.transform;
    const w = this.world;

    // ── olhar: mouse (delta) + setas ────────────────────────────────────────
    const mdx = this.input.mouseDX();
    const mdy = this.input.mouseDY();
    const ah = this.input.arrowH();
    const av = this.input.arrowV();
    t.yaw = t.yaw + mdx * this.lookSens + ah * 2.1 * dt;
    t.pitch = t.pitch - mdy * this.lookSens + av * 2.1 * dt;
    if (t.pitch > 1.45) t.pitch = 1.45;
    if (t.pitch < 0 - 1.45) t.pitch = 0 - 1.45;

    // ── andar (eixos estilo Unity, relativos ao yaw) ────────────────────────
    const axH = this.input.axisH();
    const axV = this.input.axisV();
    const fxw = math.sin(t.yaw);
    const fzw = math.cos(t.yaw);
    const rxw = math.cos(t.yaw);
    const rzw = 0 - math.sin(t.yaw);
    const mvx = (axV * fxw + axH * rxw) * this.speed * dt;
    const mvz = (axV * fzw + axH * rzw) * this.speed * dt;

    // ── na água? (bloco nos pés) ────────────────────────────────────────────
    const wb = w.get(math.floor(t.x), math.floor(t.y - 1.2), math.floor(t.z));
    this.inWater = 0;
    if (wb === 4) this.inWater = 1;

    // ── colisão por eixo (pés + olhos no destino) ───────────────────────────
    if (mvx !== 0) {
      let sgx: f64 = 0.3;
      if (mvx < 0) sgx = 0 - 0.3;
      const nx = t.x + mvx;
      const cbx = math.floor(nx + sgx);
      const cbz = math.floor(t.z);
      const sFeet = w.solid(cbx, math.floor(t.y - 1.4), cbz);
      const sEye = w.solid(cbx, math.floor(t.y + 0.2), cbz);
      let blocked = 0;
      if (cbx < 1 || cbx >= w.sx - 1) blocked = 1;
      if (sFeet !== 0) blocked = 1;
      if (sEye !== 0) blocked = 1;
      if (blocked === 0) t.x = nx;
    }
    if (mvz !== 0) {
      let sgz: f64 = 0.3;
      if (mvz < 0) sgz = 0 - 0.3;
      const nz = t.z + mvz;
      const cbx2 = math.floor(t.x);
      const cbz2 = math.floor(nz + sgz);
      const sFeet2 = w.solid(cbx2, math.floor(t.y - 1.4), cbz2);
      const sEye2 = w.solid(cbx2, math.floor(t.y + 0.2), cbz2);
      let blocked2 = 0;
      if (cbz2 < 1 || cbz2 >= w.sz - 1) blocked2 = 1;
      if (sFeet2 !== 0) blocked2 = 1;
      if (sEye2 !== 0) blocked2 = 1;
      if (blocked2 === 0) t.z = nz;
    }

    // ── gravidade / pulo / nado ─────────────────────────────────────────────
    const jp = this.input.jump();
    if (this.inWater !== 0) {
      this.vy = this.vy * 0.9 - 3.0 * dt;
      if (jp !== 0) this.vy = 2.6;
    } else {
      this.vy = this.vy - 17.0 * dt;
      if (this.grounded !== 0 && jp !== 0) { this.vy = 6.2; this.grounded = 0; }
    }
    const ny = t.y + this.vy * dt;
    if (this.vy < 0) {
      const fby = math.floor(ny - 1.62);
      const sFloor = w.solid(math.floor(t.x), fby, math.floor(t.z));
      let hitFloor = 0;
      if (sFloor !== 0) hitFloor = 1;
      if (fby < 0) hitFloor = 1;
      if (hitFloor !== 0) {
        t.y = fby + 2.62;
        this.vy = 0;
        this.grounded = 1;
      } else {
        t.y = ny;
        this.grounded = 0;
      }
    } else if (this.vy > 0) {
      const sCeil = w.solid(math.floor(t.x), math.floor(ny + 0.3), math.floor(t.z));
      if (sCeil !== 0) this.vy = 0;
      else t.y = ny;
      this.grounded = 0;
    }
  }
}
