// Guards the CLASS-PROTOTYPE HOIST gate (`front/run/class/protohoist.rs`).
//
// The engine wires a class's shared prototype ONCE in the function's entry block
// instead of on every `new`, which turns `1 + method_count` extern calls per
// CONSTRUCTION into the same number per FUNCTION (~11x on a 4-method class built
// in a loop — see docs/specs/FUTURE_OPTIMIZATION.md).
//
// That hoist is only sound while nothing replaces a prototype: the runtime's
// `__rtsadp_class_proto_init` defers its [[Prototype]] chain link to the first
// `new` ON PURPOSE, so that a `F.prototype = Object.create(Base.prototype)`
// executed beforehand survives. Hoisting past such an assignment silently loses
// it — instances read `undefined` for every inherited key. So any `.prototype`
// write anywhere in the program disables the hoist program-wide.
//
// This file pins both halves of that contract in ONE program: a prototype
// replacement (which must keep working) sitting alongside a class constructed in
// a loop (which is what the hoist targets). If the gate is ever dropped or
// narrowed, the chain assertions below go undefined.

import { describe, test, expect } from "rts:test";

// --- half 1: the prototype replacement the gate exists to protect -------------
function Base(): void {}
Base.prototype.kind = "base" as any;

function Derived(): void {}
Derived.prototype = Object.create(Base.prototype as any) as any;
Derived.prototype.own = "derived" as any;

const d = new (Derived as any)();
const inherited: string = (d as any).kind;
const own: string = (d as any).own;

// --- half 2: a real class constructed in a loop, in the SAME program ----------
class Point {
  x: number;
  y: number;
  constructor(x: number, y: number) {
    this.x = x;
    this.y = y;
  }
  norm2(): number { return this.x * this.x + this.y * this.y; }
  sum(): number { return this.x + this.y; }
}

let acc = 0;
let lastSum = 0;
for (let i = 0; i < 50; i++) {
  const p = new Point(i, i + 1);
  acc = acc + p.norm2();
  lastSum = p.sum();
}

// Method dispatch must still work through the prototype after all of the above.
const single = new Point(3, 4);

describe("class prototype hoist gate", () => {
  test("a replaced prototype still reaches instances (chain preserved)", () => {
    expect(inherited).toBe("base");
    expect(own).toBe("derived");
  });

  test("a class built in a loop keeps correct per-instance state", () => {
    // sum over i in 0..49 of (i^2 + (i+1)^2)
    //   sum i^2,     i = 0..49 = 49*50*99/6  = 40425
    //   sum (i+1)^2, i = 0..49 = 50*51*101/6 = 42925
    expect(`${acc}`).toBe(`${83350}`);
    // the LAST iteration's instance: i = 49 → 49 + 50
    expect(`${lastSum}`).toBe(`${99}`);
  });

  test("method dispatch works after the loop", () => {
    expect(`${single.norm2()}`).toBe(`${25}`);
    expect(`${single.sum()}`).toBe(`${7}`);
  });

  test("instances are distinct objects, not a shared one", () => {
    const a = new Point(1, 1);
    const b = new Point(2, 2);
    expect(`${a.sum()}`).toBe(`${2}`);
    expect(`${b.sum()}`).toBe(`${4}`);
  });
});
