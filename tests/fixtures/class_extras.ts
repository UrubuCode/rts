import { io } from "rts";

// super.method()
class Base {
  hi(): void { io.print("base"); }
}
class Mid extends Base {
  hi(): void {
    super.hi();
    io.print("mid");
  }
}
class Top extends Mid {
  hi(): void {
    super.hi();
    io.print("top");
  }
}
new Top().hi();
io.print("---");

// static methods
class Util {
  static double(n: i32): i32 { return n * 2; }
  static add(a: i32, b: i32): i32 { return a + b; }
}
io.print(`double=${Util.double(7)}`);
io.print(`add=${Util.add(3, 4)}`);

// getters / setters
class Box {
  _value: i32;
  constructor(v: i32) { this._value = v; }
  get value(): i32 { return this._value; }
  set value(v: i32) {
    if (v < 0) {
      this._value = 0;
    } else {
      this._value = v;
    }
  }
}
const b: Box = new Box(10);
io.print(`init=${b.value}`);
b.value = 50;
io.print(`set50=${b.value}`);
b.value = -5;
io.print(`clamped=${b.value}`);

// computed getter
class Rect {
  w: i32;
  h: i32;
  constructor(w: i32, h: i32) {
    this.w = w;
    this.h = h;
  }
  get area(): i32 { return this.w * this.h; }
}
const r: Rect = new Rect(3, 4);
io.print(`area=${r.area}`);

// empty body method
class NoOp {
  do(): void {}
}
new NoOp().do();
io.print("empty-method-ok");
