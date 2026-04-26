import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

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
    print(`(${this.x}, ${this.y})`);
  }
}

const p: Point = new Point(3, 4);
print(`sum=${p.sum()}`);
p.describe();

const q: Point = new Point(10, 20);
print(`q.x=${q.x} q.sum=${q.sum()}`);

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
print(`counter=${c.value()}`);

describe("fixture:class_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("sum=7\n(3, 4)\nq.x=10 q.sum=30\ncounter=3\n");
  });
});
