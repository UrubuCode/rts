// Operator overloading in RTS: opt-in, by symbol.
//
//   target/release/rts.exe run examples/claude-operadores.ts
//
// Only an object that declares a symbol from `operators` is overloaded.
// Everything else (Set, Map, a class with a method NAMED `add`, `valueOf`)
// behaves exactly as it does in Node and Bun.
// `tsc` and the editor still flag `a + b` between classes: TypeScript has no
// syntax for declaring this. RTS runs it because it does not type-check.

import { operators } from "rts";

// ---------------------------------------------------------------- 1. vectors
class Vec2 {
  constructor(public x: number, public y: number) {}
  [operators.add](o: Vec2) { return new Vec2(this.x + o.x, this.y + o.y); }
  [operators.sub](o: Vec2) { return new Vec2(this.x - o.x, this.y - o.y); }
  // `reversed` is true when the vector is on the RIGHT: `5 * v`.
  [operators.mul](k: number, _reversed: boolean) { return new Vec2(this.x * k, this.y * k); }
  [operators.neg]() { return new Vec2(-this.x, -this.y); }
  [operators.eq](o: Vec2) { return this.x === o.x && this.y === o.y; }
  toString() { return `(${this.x}, ${this.y})`; }
}

let pos = new Vec2(0, 0);
const vel = new Vec2(3, 4);
const dt = 0.5;
for (let t = 0; t < 4; t++) pos += vel * dt; // the physics line, written the way it reads
console.log("1. pos + vel*dt, 4 steps  =", `${pos}`);
console.log("   5 * vel (reflected)    =", `${5 * vel}`);
console.log("   -vel                   =", `${-vel}`);
console.log("   vel == new Vec2(3,4)   =", vel == new Vec2(3, 4));
console.log("   vel === new Vec2(3,4)  =", vel === new Vec2(3, 4), "(=== is identity, never overloaded)");

// ------------------------------------------------- 2. money without float error
class Money {
  constructor(public cents: bigint) {}
  static of(s: string) {
    const [a, b = "0"] = s.split(".");
    return new Money(BigInt(a) * 100n + BigInt((b + "00").slice(0, 2)));
  }
  [operators.add](o: Money) { return new Money(this.cents + o.cents); }
  [operators.mul](k: number) { return new Money(this.cents * BigInt(k)); }
  [operators.lt](o: Money) { return this.cents < o.cents; }
  toString() {
    const c = this.cents;
    return `R$ ${c / 100n}.${String(c % 100n).padStart(2, "0")}`;
  }
}
const price = Money.of("0.10");
const shipping = Money.of("0.20");
const total = price + shipping;
console.log("2. 0.10 + 0.20 (Money)    =", `${total}`, "  plain float:", 0.1 + 0.2);
console.log("   R$ 0.10 * 3 + frete    =", `${price * 3 + shipping}`);
console.log("   total < R$ 1.00        =", total < Money.of("1.00"));

// ------------------------------------------------------------------ 3. units
class Meters {
  constructor(public v: number) {}
  [operators.add](o: Meters | number, reversed: boolean) {
    const other = o instanceof Meters ? o.v : o;
    return new Meters(reversed ? other + this.v : this.v + other);
  }
  [operators.gt](o: Meters) { return this.v > o.v; }
  toString() { return `${this.v} m`; }
}
const cm = (n: number) => new Meters(n / 100);
console.log("3. 5 m + 20 cm            =", `${new Meters(5) + cm(20)}`);
console.log("   2 + 3 m (reflected)    =", `${2 + new Meters(3)}`);
console.log("   5 m > 20 cm            =", new Meters(5) > cm(20));

// ------------------------------------- 4. nothing changes for anyone else
const s = new Set([1, 2]);
class HasAdd { add(x: number) { return "WRONG: add was called " + x; } }
const withValueOf = { valueOf() { return 42; } };
console.log("4. Set + ''               =", JSON.stringify(s + ""), "(like Node: .add is not called)");
console.log("   HasAdd + 1             =", JSON.stringify(new HasAdd() + 1));
console.log("   valueOf + 1            =", withValueOf + 1);
console.log("   1 + 2                  =", 1 + 2);
console.log("   vel == null            =", vel == null, "(eq only between two objects)");

// ----------------------------------- 5. an exception inside the operator
class Strict {
  [operators.add](): never { throw new RangeError("adding a Strict is forbidden"); }
}
try {
  new Strict() + new Strict();
} catch (e) {
  console.log("5. exception in operator  =", (e as Error).name + ":", (e as Error).message);
}

// ------------------------------- 6. inheritance: the symbol comes with the class
class Vec3 extends Vec2 {}
console.log("6. Vec3 inherits add      =", `${new Vec3(1, 1) + new Vec2(1, 1)}`);
