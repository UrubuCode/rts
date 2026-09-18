import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

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
    print(`(${this.x}, ${this.y})`);
  }
}

const a: Vec2 = new Vec2(1, 2);
const b: Vec2 = new Vec2(3, 4);

// JavaScript has no operator overloading for classes — confirmed against
// Node v20 (`node -e`): `a + b` for two plain objects never calls a method
// named `add`, it runs ToPrimitive (no `valueOf`/`Symbol.toPrimitive` here,
// so the default — string concatenation of "[object Object]" twice) exactly
// like `a - b` would run `ToNumber` and answer `NaN`. This file used to
// write `a + b` and assert it returned a `Vec2`, which is not something any
// JS engine does; the previous "pass" was this engine wrongly special-casing
// `+`/`-`/`*`/`==` between two class instances into a method dispatch. That
// was reverted (see class methods below, called explicitly) and this test
// now pins BOTH the real way to combine two `Vec2`s (an ordinary method) and
// the real behaviour of the bare operator (default `ToPrimitive`).
const c: Vec2 = a.add(b);
c.describe();

const d: Vec2 = b.sub(a);
d.describe();

const e: Vec2 = a.mul(5);
e.describe();

const f: Vec2 = new Vec2(4, 6);
print(`c == f: ${c.eq(f)}`);
print(`c == a: ${c.eq(a)}`);

// The raw operator, unmodified: default `ToPrimitive` string concatenation,
// same as Node.
print(`a + b (raw): ${a + b}`);

describe("fixture:operator_overload", () => {
  test("no operator overloading in JS — .add/.sub/.mul/.eq are ordinary methods, `+` is default ToPrimitive", () => {
    expect(__rtsCapturedOutput).toBe(
      "(4, 6)\n(2, 2)\n(5, 10)\nc == f: 1\nc == a: 0\na + b (raw): [object Object][object Object]\n"
    );
  });
});
