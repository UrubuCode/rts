import { io } from "rts";

class Vec2 {
  x: i32;
  y: i32;
  constructor(x: i32, y: i32) {
    this.x = x;
    this.y = y;
  }
  add(other: Vec2): Vec2 {
    return new Vec2(this.x + other.x, this.y + other.y);
  }
  sub(other: Vec2): Vec2 {
    return new Vec2(this.x - other.x, this.y - other.y);
  }
  mul(k: i32): Vec2 {
    return new Vec2(this.x * k, this.y * k);
  }
  eq(other: Vec2): i32 {
    return this.x == other.x && this.y == other.y ? 1 : 0;
  }
  describe(): void {
    io.print(`(${this.x}, ${this.y})`);
  }
}

const a: Vec2 = new Vec2(1, 2);
const b: Vec2 = new Vec2(3, 4);

const c: Vec2 = a + b;
c.describe();

const d: Vec2 = b - a;
d.describe();

const e: Vec2 = a * 5;
e.describe();

const f: Vec2 = new Vec2(4, 6);
io.print(`c == f: ${c == f}`);
io.print(`c == a: ${c == a}`);
