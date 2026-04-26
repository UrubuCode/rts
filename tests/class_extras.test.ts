import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// super.method()
class Base {
  hi(): void { print("base"); }
}
class Mid extends Base {
  hi(): void {
    super.hi();
    print("mid");
  }
}
class Top extends Mid {
  hi(): void {
    super.hi();
    print("top");
  }
}
new Top().hi();
print("---");

// static methods
class Util {
  static double(n: i32): i32 { return n * 2; }
  static add(a: i32, b: i32): i32 { return a + b; }
}
print(`double=${Util.double(7)}`);
print(`add=${Util.add(3, 4)}`);

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
print(`init=${b.value}`);
b.value = 50;
print(`set50=${b.value}`);
b.value = -5;
print(`clamped=${b.value}`);

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
print(`area=${r.area}`);

// empty body method
class NoOp {
  do(): void {}
}
new NoOp().do();
print("empty-method-ok");

describe("fixture:class_extras", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("base\nmid\ntop\n---\ndouble=14\nadd=7\ninit=10\nset50=50\nclamped=0\narea=12\nempty-method-ok\n");
  });
});
