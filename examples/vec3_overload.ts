import { io } from "rts";

class Vec3 {
  x: i32;
  y: i32;
  z: i32;

  constructor(x: i32, y: i32, z: i32) {
    this.x = x;
    this.y = y;
    this.z = z;
  }

  add(other: Vec3): Vec3 {
    return new Vec3(this.x + other.x, this.y + other.y, this.z + other.z);
  }

  sub(other: Vec3): Vec3 {
    return new Vec3(this.x - other.x, this.y - other.y, this.z - other.z);
  }

  mul(k: i32): Vec3 {
    return new Vec3(this.x * k, this.y * k, this.z * k);
  }

  eq(other: Vec3): i32 {
    return this.x == other.x && this.y == other.y && this.z == other.z ? 1 : 0;
  }

  describe(): void {
    io.print(`(${this.x}, ${this.y}, ${this.z})`);
  }
}

const origem: Vec3 = new Vec3(0, 0, 0);
const ponto: Vec3 = new Vec3(3, 4, 5);
const direcao: Vec3 = new Vec3(1, 1, 1);

const destino: Vec3 = ponto + direcao;
const delta: Vec3 = destino - origem;

destino.describe();
delta.describe();

const dobro: Vec3 = ponto * 2;
dobro.describe();

const a: Vec3 = new Vec3(1, 2, 3);
const b: Vec3 = new Vec3(1, 2, 3);
const c: Vec3 = new Vec3(9, 9, 9);
io.print(`a == b: ${a == b}`);
io.print(`a == c: ${a == c}`);

const resultado: Vec3 = (ponto + direcao) - origem;
resultado.describe();
