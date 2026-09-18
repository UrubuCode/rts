import { describe, test, expect } from "rts:test";
import { operators } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Operator overloading is OPT-IN, by symbols only `rts` hands out
// (docs/engine/operator-overloading.md). The engine this one replaced turned
// `a + b` into `a.add(b)` whenever a method called `add` existed — which is
// not JavaScript, and broke `set + ""` for every `Set`. Here a class answers an
// operator only if it declares `[operators.add]`; one that does not is
// answered by the specification, exactly as Node answers it.
class Vec2 {
  x: number;
  y: number;
  constructor(x: number, y: number) {
    this.x = x;
    this.y = y;
  }
  [operators.add](other: Vec2, reversed: boolean): Vec2 {
    return new Vec2(this.x + other.x, this.y + other.y);
  }
  [operators.sub](other: Vec2, reversed: boolean): Vec2 {
    return reversed
      ? new Vec2(other.x - this.x, other.y - this.y)
      : new Vec2(this.x - other.x, this.y - other.y);
  }
  // `a * 5` asks `a` with `reversed = false`; `5 * a` asks `a` with
  // `reversed = true`, because a number declares nothing.
  [operators.mul](k: number, reversed: boolean): Vec2 {
    return new Vec2(this.x * k, this.y * k);
  }
  [operators.eq](other: Vec2, reversed: boolean): boolean {
    return this.x === other.x && this.y === other.y;
  }
  describe(): void {
    print(`(${this.x}, ${this.y})`);
  }
}

// The same fields and no symbol: the operator is JavaScript's.
class Plain {
  x: number;
  constructor(x: number) {
    this.x = x;
  }
}

const a = new Vec2(1, 2);
const b = new Vec2(3, 4);

const c: Vec2 = (a as any) + (b as any);
c.describe();

const d: Vec2 = (b as any) - (a as any);
d.describe();

const e: Vec2 = (a as any) * 5;
e.describe();

const g: Vec2 = 5 * (a as any);
g.describe();

const f = new Vec2(4, 6);
print(`c == f: ${(c as any) == (f as any)}`);
print(`c == a: ${(c as any) == (a as any)}`);
print(`c != a: ${(c as any) != (a as any)}`);
// `===` is identity, never overloaded.
print(`c === f: ${c === f}`);

// A class WITHOUT the symbol: default `ToPrimitive`, same as Node.
const p = new Plain(1);
const q = new Plain(2);
print(`plain + plain: ${(p as any) + (q as any)}`);

describe("fixture:operator_overload", () => {
  test("[operators.*] answers + - * == for Vec2; a class without it keeps ToPrimitive", () => {
    expect(__rtsCapturedOutput).toBe(
      "(4, 6)\n(2, 2)\n(5, 10)\n(5, 10)\n" +
        "c == f: true\nc == a: false\nc != a: true\nc === f: false\n" +
        "plain + plain: [object Object][object Object]\n"
    );
  });
});
