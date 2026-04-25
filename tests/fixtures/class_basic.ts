import { io } from "rts";

class Point {
  x: i32;
  y: i32;
  constructor(x: i32, y: i32) {
    this.x = x;
    this.y = y;
  }
  sum(): i32 {
    return this.x + this.y;
  }
  describe(): void {
    io.print(`(${this.x}, ${this.y})`);
  }
}

const p: Point = new Point(3, 4);
io.print(`sum=${p.sum()}`);
p.describe();

const q: Point = new Point(10, 20);
io.print(`q.x=${q.x} q.sum=${q.sum()}`);

class Counter {
  n: i32;
  constructor() {
    this.n = 0;
  }
  inc(): void {
    this.n = this.n + 1;
  }
  value(): i32 {
    return this.n;
  }
}

const c: Counter = new Counter();
c.inc();
c.inc();
c.inc();
io.print(`counter=${c.value()}`);
